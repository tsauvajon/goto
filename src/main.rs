/*!
shorturl is a web server that can host shortened URLs.

## Example usage

Creating a link:
```
$ curl -X POST 127.0.0.1:8080/tsauvajon -d "https://linkedin.com/in/tsauvajon"
/tsauvajon now redirects to https://linkedin.com/in/tsauvajon
```

Using it redirects us:
```
$ curl 127.0.0.1:8080/tsauvajon -v
*   Trying 127.0.0.1...
* TCP_NODELAY set
* Connected to 127.0.0.1 (127.0.0.1) port 8080 (#0)
> GET /tsauvajon HTTP/1.1
> Host: 127.0.0.1:8080
> User-Agent: curl/7.64.1
> Accept: * / *
>
< HTTP/1.1 302 Found
< content-length: 51
< location: https://linkedin.com/in/tsauvajon
< date: Wed, 19 May 2021 17:36:49 GMT
<
* Connection #0 to host 127.0.0.1 left intact
redirecting to https://linkedin.com/in/tsauvajon ...* Closing connection 0
```
*/

#![deny(
    warnings,
    missing_doc_code_examples,
    missing_docs,
    clippy::all,
    clippy::cargo
)]

use actix_files::{Files, NamedFile};
use actix_web::{error, get, post, web, App, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use structopt::StructOpt;
use url::Url;

const MAX_SIZE: usize = 1_024; // max payload size is 1k
const RANDOM_URL_SIZE: usize = 5; // ramdomly generated URLs are 5 characters long

// struct Data(HashMap<String, String>);
struct Data {
    data: HashMap<String, String>,
    persistence: Option<Arc<RwLock<File>>>,
}

impl Data {
    fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    fn contains_key(&self, key: &String) -> bool {
        self.data.contains_key(key)
    }

    fn insert(&mut self, key: String, value: String) -> Option<String> {
        match self.data.insert(key.clone(), value.clone()) {
            Some(existing_value) => Some(existing_value),
            None => {
                if let Some(persistence) = &self.persistence {
                    let mut file = persistence.write().unwrap();
                    file.write_all(serialise_entry(key, value).as_bytes())
                        .expect("persist new entry");
                }
                None
            }
        }
    }

    fn new(data: HashMap<String, String>) -> Self {
        Data {
            data,
            persistence: None,
        }
    }

    fn with_persistence(mut self, persistence: Arc<RwLock<File>>) -> Self {
        self.persistence = Some(persistence);
        self
    }
}

#[derive(Clone)]
struct Db {
    data: web::Data<RwLock<Data>>,
}

impl Db {
    fn read(
        &self,
    ) -> Result<
        std::sync::RwLockReadGuard<Data>,
        std::sync::PoisonError<std::sync::RwLockReadGuard<Data>>,
    > {
        self.data.read()
    }

    fn write(
        &self,
    ) -> Result<
        std::sync::RwLockWriteGuard<Data>,
        std::sync::PoisonError<std::sync::RwLockWriteGuard<Data>>,
    > {
        self.data.write()
    }

    fn new(data: Data) -> Self {
        Db {
            data: web::Data::new(RwLock::new(data)),
        }
    }
}

/// serialise_entry serialises a new database entry into
/// a new YAML line, that can be added to an existing
/// database.
fn serialise_entry(key: String, value: String) -> String {
    format!("{}: \"{}\"\n", key, value)
}

/// browse redirects to the long URL hidden behind a short URL, or returns a
/// 404 not found error if the short URL doesn't exist.
#[get("/{id}")]
async fn browse(db: web::Data<Db>, web::Path(id): web::Path<String>) -> impl Responder {
    match db.read() {
        Ok(db) => match db.get(&id) {
            None => Err(error::ErrorNotFound("not found")),
            Some(url) => Ok(HttpResponse::Found()
                .header("Location", url.clone())
                .body(format!("redirecting to {} ...", url))),
        },
        Err(err) => {
            println!("accessing the db: {}", err);
            Err(error::ErrorInternalServerError(err.to_string()))
        }
    }
}

/// hash returns a short hash of the string passed as a parameter.
fn hash(input: &str) -> String {
    blake3::hash(input.as_bytes()).to_hex()[..RANDOM_URL_SIZE].to_string()
}

/// Read a string target from an actix_web Payload
async fn read_target(mut payload: web::Payload) -> Result<String, String> {
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.or_else(|err| Err(err.to_string()))?;
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err("overflow".to_string());
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body[..].to_vec())
        .or_else(|err| Err(format!("invalid request body: {}", err)))
}

/// Create an short URL redirecting to a long URL.
/// If you pass an `id` a parameter, your short URL will be /{id}.
/// If you pass `None` instead, it will be /{hash of the target URL}.
fn create_short_url(
    db: web::Data<Db>,
    target: String,
    id: Option<String>,
) -> Result<String, String> {
    if let Err(err) = Url::parse(&target) {
        return Err(format!("malformed URL: {}", err));
    };

    let id = match id {
        Some(id) => id,
        None => hash(&target),
    };

    let mut db = db.write().unwrap();
    if db.contains_key(&id) {
        Err("already registered".to_string())
    } else {
        db.insert(id.clone(), target.clone());
        Ok(format!("/{} now redirects to {}", id, target))
    }
}

#[post("/{id}")]
async fn create_with_id(
    db: web::Data<Db>,
    payload: web::Payload,
    web::Path(id): web::Path<String>,
) -> impl Responder {
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    create_short_url(db, target, Some(id)).or_else(|err| Err(error::ErrorBadRequest(err)))
}

#[post("/")]
async fn create_random(db: web::Data<Db>, payload: web::Payload) -> impl Responder {
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    create_short_url(db, target, None).or_else(|err| Err(error::ErrorBadRequest(err)))
}

#[get("/")]
async fn index() -> std::io::Result<NamedFile> {
    let path: PathBuf = "/etc/shorturl/dist/index.html".parse().unwrap();
    Ok(NamedFile::open(path)?)
}

#[derive(StructOpt)]
struct Cli {
    #[structopt(short = "f", long = "frontdir")]
    front_dist_directory: Option<String>,

    #[structopt(short = "p", long = "port")]
    port: Option<usize>,

    #[structopt(short = "d", long = "database")]
    database: Option<String>,
}

fn open_db(args: &Cli) -> Db {
    let data = match &args.database {
        None => Data::new(HashMap::new()),
        Some(path) => {
            let path = std::path::Path::new(&path);

            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .read(true)
                .truncate(false)
                .open(path.clone());

            let mut database = match file {
                Ok(file) => file,
                Err(_) => {
                    println!("creating database at {:?}", path.clone());
                    let path = std::path::Path::new(&path);
                    let prefix = path.parent().unwrap();
                    std::fs::create_dir_all(prefix).expect("create folder structure");
                    File::create(path).expect("create database")
                }
            };

            let mut buf = String::new();
            match database.read_to_string(&mut buf) {
                Err(_) => Data::new(HashMap::new()),
                Ok(len) => {
                    if len == 0 {
                        Data::new(HashMap::new()).with_persistence(Arc::new(RwLock::new(database)))
                    } else {
                        let yaml_contents: HashMap<String, String> =
                            serde_yaml::from_str(&buf).expect("read database");

                        Data::new(yaml_contents.into())
                            .with_persistence(Arc::new(RwLock::new(database)))
                    }
                }
            }
        }
    };

    Db::new(data)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Cli::from_args();
    let front_dist_directory = match &args.front_dist_directory {
        Some(dir) => (*dir).clone(),
        None => "front/dist/".to_string(),
    };

    let addr = match &args.port {
        Some(port) => format!("127.0.0.1:{}", port),
        None => "127.0.0.1:8080".to_string(),
    };

    let db = open_db(&args);

    HttpServer::new(move || {
        App::new()
            .service(index)
            .service(Files::new("/dist", front_dist_directory.clone()))
            .data(db.clone())
            .service(browse)
            .service(create_random)
            .service(create_with_id)
    })
    .bind(addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash() {
        assert_eq!("4cca4", hash("something"));
        assert_eq!("284a1", hash("something else"));
    }

    #[test]
    fn test_create_short_malformed_url() {
        let db: Db = Db::new(Data::new(HashMap::new()));

        let target = "this is not a valid URL".to_string();
        let id = Some("hello".to_string());
        assert_eq!(
            Err("malformed URL: relative URL without a base".to_string()),
            create_short_url(web::Data::new(db), target, id)
        );
    }

    #[test]
    fn test_create_short_url() {
        let db: Db = Db::new(Data::new(HashMap::new()));

        let target = "https://google.com".to_string();
        let id = "hello".to_string();
        create_short_url(web::Data::new(db.clone()), target.clone(), Some(id.clone())).unwrap();

        let db = db.read().unwrap();
        let got = db.get(&id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_hashed_id() {
        let db: Db = Db::new(Data::new(HashMap::new()));

        let target = "https://google.com";
        create_short_url(web::Data::new(db.clone()), target.to_string(), None).unwrap();

        let id = hash(target);
        let db = db.read().unwrap();
        let got = db.get(&id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_already_exists() {
        let id = "hello".to_string();

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert(id.clone(), "some existing value".to_string());
        let db: Db = Db::new(Data::new(db.into()));

        let target = "https://google.com".to_string();
        assert_eq!(
            Err("already registered".to_string()),
            create_short_url(web::Data::new(db), target, Some(id))
        );
    }

    #[test]
    fn test_read_database() {
        extern crate serde_yaml;
        let data = "hello: http://hello-world.com\nkey2: value2";

        let yaml_contents: HashMap<String, String> = serde_yaml::from_str(&data).unwrap();
        println!("{:?}", yaml_contents);
    }

    #[test]
    // We write new database (= file) lines one at a time, and serde_yaml
    // to_string method doesn't help for two reasons:
    //   - we don't need the error handling
    //   - we don't want the `---\n` prefix
    //
    // On the other hand, if we wanted to write the entire database every
    // time, it would work well.
    fn test_write_database() {
        extern crate serde_yaml;
        let mut database: HashMap<String, String> = HashMap::new();
        database.insert(
            "tsauvajon".to_string(),
            "https://linkedin.com/in/tsauvajon".to_string(),
        );
        let want = serde_yaml::to_string(&database).unwrap();
        let want = want.trim_start_matches("---\n").to_string();

        let got = serialise_entry(
            "tsauvajon".to_string(),
            "https://linkedin.com/in/tsauvajon".to_string(),
        );

        assert_eq!(want, got)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use actix_web::{
        body::Body,
        http::{HeaderValue, StatusCode},
        test,
    };

    // create a new custom shorturl
    #[actix_rt::test]
    async fn integration_test_create_custom_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/hello")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = Db::new(Data::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create_with_id)).await;
        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let db = db.read().unwrap();
        assert_eq!(db.get("hello"), Some(&"https://hello.world".to_string()));
        assert_eq!(db.get("wwerwewrew"), None);
    }

    // create a new random shorturl
    #[actix_rt::test]
    async fn integration_test_create_random_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = Db::new(Data::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create_random)).await;
        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let db = db.read().unwrap();
        assert_eq!(
            db.get(&hash("https://hello.world")),
            Some(&"https://hello.world".to_string())
        );
        assert_eq!(db.get("wwerwewrew"), None);
    }

    // follow an existing shorturl
    #[actix_rt::test]
    async fn integration_test_use_shortened_url() {
        let req = test::TestRequest::get().uri("/hi").to_request();

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert("hi".into(), "https://linkedin.com/in/tsauvajon".into());

        let db: Db = Db::new(Data::new(db.into()));

        let mut app = test::init_service(App::new().data(db).service(browse)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::FOUND);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(
            &Body::from("redirecting to https://linkedin.com/in/tsauvajon ..."),
            body
        );

        assert_eq!(
            resp.headers().get("Location"),
            Some(&HeaderValue::from_str("https://linkedin.com/in/tsauvajon").unwrap())
        )
    }

    // try to follow a shortened URL that doesn't exist
    #[actix_rt::test]
    async fn integration_test_link_miss() {
        let req = test::TestRequest::get()
            .uri("/thislinkdoesntexist")
            .to_request();

        let db: Db = Db::new(Data::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db).service(browse)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(&Body::from("not found"), body);

        assert_eq!(resp.headers().get("Location"), None)
    }

    // try to add a link for an already existing short-url
    #[actix_rt::test]
    async fn integration_test_collision() {
        let req = test::TestRequest::post()
            .uri("/alreadyexists")
            .set_payload("https://something.new")
            .to_request();

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert(
            "alreadyexists".into(),
            "https://github.com/tsauvajon".into(),
        );

        let db: Db = Db::new(Data::new(db.into()));
        let mut app = test::init_service(App::new().data(db).service(create_with_id)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(&Body::from("already registered"), body);
    }
}
