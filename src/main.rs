/*!
goto is a web server that can create shortened URLs.

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

use actix_files::Files;
use actix_web::{error, get, post, web, App, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::RwLock;
use structopt::StructOpt;
use url::Url;

const MAX_SIZE: usize = 256; // max payload size is 256 Kb
const RANDOM_URL_SIZE: usize = 5; // ramdomly generated URLs are 5 characters long

struct Data {
    data: HashMap<String, String>,
    persistence: Option<File>,
}

impl Data {
    fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn insert(&mut self, key: &str, value: &str) -> Option<String> {
        match self.data.insert(key.to_string(), value.to_string()) {
            Some(existing_value) => Some(existing_value),
            None => {
                if let Some(file) = &mut self.persistence {
                    file.write_all(serialise_entry(key.to_string(), value.to_string()).as_bytes())
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

    fn with_persistence(mut self, persistence: File) -> Self {
        self.persistence = Some(persistence);
        self
    }
}

#[test]
fn test_insert_data() {
    use std::env::temp_dir;

    let dir = temp_dir();
    let tmpfile_path = format!("{}/tmpfile2.txt", dir.to_str().unwrap());
    let file = File::create(&tmpfile_path).unwrap();

    {
        let mut data = Data::new(HashMap::new()).with_persistence(file);
        let outcome = data.insert("hi", "qwerty");
        assert_eq!(None, outcome);

        let outcome = data.insert("hi", "zxcvbnm");
        assert_eq!(Some("qwerty".to_string()), outcome);
    }

    let mut file = File::open(tmpfile_path).unwrap();
    let mut got = String::new();
    file.read_to_string(&mut got).unwrap();

    assert_eq!("hi: \"qwerty\"\n".to_string(), got);
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
                .header("Location", url.to_string())
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
fn create_short_url(db: web::Data<Db>, target: &str, id: Option<&str>) -> Result<String, String> {
    if let Err(err) = Url::parse(&target) {
        return Err(format!("malformed URL: {}", err));
    };

    let id = match id {
        Some(id) => id.to_string(),
        None => hash(&target),
    };

    let mut db = db.write().unwrap();
    if db.contains_key(&id) {
        Err("already registered".to_string())
    } else {
        db.insert(&id, target);
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

    create_short_url(db, &target, Some(id.as_str())).or_else(|err| Err(error::ErrorBadRequest(err)))
}

#[post("/")]
async fn create_random(db: web::Data<Db>, payload: web::Payload) -> impl Responder {
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    create_short_url(db, &target, None).or_else(|err| Err(error::ErrorBadRequest(err)))
}

#[derive(StructOpt)]
struct Cli {
    #[structopt(short = "f", long = "frontdir")]
    /// Directory where the front-end files are located, default: "front/dist".
    front_dist_directory: Option<String>,

    #[structopt(short = "a", long = "addr")]
    /// Address to run the application on, default: "127.0.0.1:8080".
    addr: Option<String>,

    #[structopt(short = "d", long = "database")]
    /// Database file to persist the shortened URLs.
    /// Will be created if it doesn't exist.
    /// Example: database.yml.
    /// If this option is omitted, the shortened URLs will not be persisted.
    database: Option<String>,
}

impl Cli {
    fn get_front_dir(&self) -> String {
        match &self.front_dist_directory {
            Some(dir) => dir.to_owned(),
            None => "front/dist/".to_string(),
        }
    }

    fn get_addr(&self) -> String {
        match &self.addr {
            Some(addr) => addr.to_owned(),
            None => "127.0.0.1:8080".to_string(),
        }
    }

    fn open_db(&self) -> Result<Db, String> {
        let data = match &self.database {
            None => Data::new(HashMap::new()),
            Some(path) => {
                let path = std::path::Path::new(&path);

                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .read(true)
                    .truncate(false)
                    .open(path.to_owned())
                    .or_else(|err| Err(err.to_string()))?;

                let mut buf = String::new();
                match file.read_to_string(&mut buf) {
                    Err(_) => Data::new(HashMap::new()),
                    Ok(len) => {
                        if len == 0 {
                            Data::new(HashMap::new()).with_persistence(file)
                        } else {
                            let yaml_contents: HashMap<String, String> = serde_yaml::from_str(&buf)
                                .or_else(|err| Err(format!("parse data: {}", err)))?;

                            Data::new(yaml_contents).with_persistence(file)
                        }
                    }
                }
            }
        };

        Ok(Db::new(data))
    }
}

#[cfg(test)]
mod cli_tests {
    use super::Cli;

    #[test]
    fn test_get_front_dir() {
        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: None,
        };
        assert_eq!("front/dist/", cli.get_front_dir());

        let cli = Cli {
            front_dist_directory: Some("/hello/world/".into()),
            addr: None,
            database: None,
        };
        assert_eq!("/hello/world/", cli.get_front_dir());
    }

    #[test]
    fn test_get_addr() {
        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: None,
        };
        assert_eq!("127.0.0.1:8080", cli.get_addr());

        let cli = Cli {
            front_dist_directory: None,
            addr: Some("123.34.56.78:99999".into()),
            database: None,
        };
        assert_eq!("123.34.56.78:99999", cli.get_addr());
    }

    #[test]
    fn test_open_db_no_persistence() {
        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: None,
        };
        let db = cli.open_db().unwrap();
        let data = db.read().unwrap();

        assert_eq!(true, data.persistence.is_none());
    }

    #[test]
    fn test_open_db_new_file() {
        use std::env::temp_dir;

        let dir = temp_dir();
        let tmpfile_path = format!("{}/tmpfile3.txt", dir.to_str().unwrap());
        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path),
        };
        let db = cli.open_db().unwrap();
        let data = db.read().unwrap();

        match &data.persistence {
            None => panic!("expected persistence"),
            Some(file) => {
                let metadata = file.metadata().unwrap();
                assert_eq!(true, metadata.is_file());
            }
        };
    }

    #[test]
    fn test_open_db_existing_file() {
        use std::env::temp_dir;
        use std::fs::File;

        let dir = temp_dir();
        let tmpfile_path = format!("{}/tmpfile.txt", dir.to_str().unwrap());

        File::create(&tmpfile_path).unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path),
        };
        let db = cli.open_db().unwrap();
        let data = db.read().unwrap();

        assert_eq!(true, data.persistence.is_some());
    }

    #[test]
    fn test_open_db_existing_file_with_data() {
        use std::env::temp_dir;
        use std::fs::File;
        use std::io::Write;

        let dir = temp_dir();
        let tmpfile_path = format!("{}/temporary-file.txt", dir.to_str().unwrap());

        let mut file = File::create(&tmpfile_path).unwrap();
        file.write_all(b"hello: \"http://world\"\n").unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path),
        };
        let db = cli.open_db().unwrap();
        let data = db.read().unwrap();

        assert_eq!(true, data.persistence.is_some());
        assert_eq!(Some(&"http://world".to_string()), data.data.get("hello"));
    }

    #[test]
    fn test_open_db_existing_file_with_bad_data() {
        use std::env::temp_dir;
        use std::fs::File;
        use std::io::Write;

        let dir = temp_dir();
        let tmpfile_path = format!("{}/tmpfile1.txt", dir.to_str().unwrap());

        let mut file = File::create(&tmpfile_path).unwrap();
        file.write_all(b"ds;flsd'f sdl;flfs~~!./'' /sf/;dsf;lsdf")
            .unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path),
        };

        let res = cli.open_db();
        assert_eq!(true, res.is_err());
        if let Err(msg) = res {
            assert_eq!(true, msg.contains("parse data: invalid type:"));
        }
    }
}

#[actix_web::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> std::io::Result<()> {
    let args = Cli::from_args();

    let front_dist_directory = args.get_front_dir();
    let addr: String = args.get_addr();
    let db = args.open_db().expect("open db");

    println!("goto listening at http://{}/", &addr);

    HttpServer::new(move || {
        App::new()
            .service(Files::new("/dist", &front_dist_directory))
            .data(db.clone())
            .service(browse)
            .service(create_random)
            .service(create_with_id)
            // this doesn't do exactly what I need (just serve index.html
            //    on /), but I can't find a simple way of doing it.
            .service(Files::new("/", &front_dist_directory).index_file("index.html"))
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
        let id = Some("hello");
        assert_eq!(
            Err("malformed URL: relative URL without a base".to_string()),
            create_short_url(web::Data::new(db), &target, id)
        );
    }

    #[test]
    fn test_create_short_url() {
        let db: Db = Db::new(Data::new(HashMap::new()));

        let target = "https://google.com".to_string();
        let id = "hello";
        create_short_url(web::Data::new(db.clone()), &target, Some(id)).unwrap();

        let db = db.read().unwrap();
        let got = db.get(&id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_hashed_id() {
        let db: Db = Db::new(Data::new(HashMap::new()));

        let target = "https://google.com";
        create_short_url(web::Data::new(db.clone()), target, None).unwrap();

        let id = hash(target);
        let db = db.read().unwrap();
        let got = db.get(&id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_already_exists() {
        let id = "hello";

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert(id.into(), "some existing value".into());
        let db: Db = Db::new(Data::new(db.into()));

        let target = "https://google.com";
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

    #[actix_rt::test]
    async fn integration_test_create_random_shortened_url_bad_body() {
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload(vec![0, 159, 146, 150])
            .to_request();

        let db: Db = Db::new(Data::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create_random)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(
            &Body::from("invalid request body: invalid utf-8 sequence of 1 bytes from index 1"),
            body
        );
    }

    #[actix_rt::test]
    async fn integration_test_create_random_shortened_url_overflow() {
        let req = test::TestRequest::post()
            .uri("/toolong")
            .set_payload(vec![b'a'; 2000])
            .to_request();

        let db: Db = Db::new(Data::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create_with_id)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(&Body::from("overflow"), body);
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

    #[actix_rt::test]
    async fn integration_test_poisoned_mutex() {
        use std::panic;

        let req = test::TestRequest::get().uri("/hi").to_request();
        let mut db: HashMap<String, String> = HashMap::new();
        db.insert("hi".into(), "https://linkedin.com/in/tsauvajon".into());
        let db: Db = Db::new(Data::new(db.into()));

        let _result = panic::catch_unwind(|| {
            panic::set_hook(Box::new(|_info| {
                // do nothing
            }));

            // This thread will acquire the mutex first, unwrapping the result of
            // `lock` because the lock has not been poisoned.
            let _guard = db.write().unwrap();

            // This panic while holding the lock (`_guard` is in scope) will poison
            // the mutex.
            panic!();
        });

        let _ = panic::take_hook(); // remove the panic hook that mutes panics

        let mut app = test::init_service(App::new().data(db).service(browse)).await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(
            &Body::from("poisoned lock: another task failed inside"),
            body
        );
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
