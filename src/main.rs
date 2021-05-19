/**
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
redirecting to https://linkedin.com/in/tsauvajon...* Closing connection 0
```
*/
use actix_web::{error, get, post, web, App, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::RwLock;
use url::Url;

const MAX_SIZE: usize = 1_024; // max payload size is 1k
const RANDOM_URL_SIZE: usize = 5; // ramdomly generated URLs are 5 characters long

type Db = web::Data<RwLock<HashMap<String, String>>>;

#[get("/{id}")]
async fn browse(db: web::Data<Db>, web::Path(id): web::Path<String>) -> impl Responder {
    match db.read() {
        Ok(db) => match db.get(&id) {
            None => Err(error::ErrorNotFound("not found")),
            Some(url) => Ok(HttpResponse::Found()
                .header("Location", url.clone())
                .body(format!("redirecting to {}...", url))),
        },
        Err(err) => {
            println!("accessing the db: {}", err);
            Err(error::ErrorInternalServerError(err.to_string()))
        }
    }
}

fn hash(input: &str) -> String {
    blake3::hash(input.as_bytes()).to_hex()[..RANDOM_URL_SIZE].to_string()
}

fn create_short_url(
    db: web::Data<Db>,
    target: String,
    id: Option<String>,
) -> Result<String, actix_web::Error> {
    if let Err(err) = Url::parse(&target) {
        return Err(error::ErrorBadRequest(format!("malformed URL: {}", err)));
    };

    let id = match id {
        Some(id) => id,
        None => hash(&target),
    };

    let mut db = db.write().unwrap();
    if db.contains_key(&id) {
        Err(error::ErrorBadRequest("already registered"))
    } else {
        db.insert(id.clone(), target.clone());
        Ok(format!("/{} now redirects to {}", id, target))
    }
}

#[post("/{id}")]
async fn create_with_id(
    db: web::Data<Db>,
    mut payload: web::Payload,
    web::Path(id): web::Path<String>,
) -> impl Responder {
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk?;
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err(error::ErrorBadRequest("overflow"));
        }
        body.extend_from_slice(&chunk);
    }

    let target = match String::from_utf8(body[..].to_vec()) {
        Ok(target) => target,
        Err(err) => {
            return Err(error::ErrorBadRequest(format!(
                "invalid request body: {}",
                err
            )))
        }
    };

    create_short_url(db, target, Some(id))
}

#[post("/")]
async fn create_random(db: web::Data<Db>, mut payload: web::Payload) -> impl Responder {
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk?;
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err(error::ErrorBadRequest("overflow"));
        }
        body.extend_from_slice(&chunk);
    }

    let target = match String::from_utf8(body[..].to_vec()) {
        Ok(target) => target,
        Err(err) => {
            return Err(error::ErrorBadRequest(format!(
                "invalid request body: {}",
                err
            )))
        }
    };

    create_short_url(db, target, None)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db: Db = web::Data::new(RwLock::new(HashMap::new()));

    HttpServer::new(move || {
        App::new()
            .data(db.clone())
            .service(browse)
            .service(create_random)
            .service(create_with_id)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{
        body::Body,
        http::{HeaderValue, StatusCode},
        test,
    };

    #[test]
    fn test_hash() {
        assert_eq!("4cca4", hash("something"));
        assert_eq!("284a1", hash("something else"));
    }

    // create a new custom shorturl
    #[actix_rt::test]
    async fn test_create_custom_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/hello")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = web::Data::new(RwLock::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create_with_id)).await;
        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let db = db.read().unwrap();
        assert_eq!(db.get("hello"), Some(&"https://hello.world".to_string()));
        assert_eq!(db.get("wwerwewrew"), None);
    }

    // create a new random shorturl
    #[actix_rt::test]
    async fn test_create_random_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = web::Data::new(RwLock::new(HashMap::new()));

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
    async fn test_use_shortened_url() {
        let req = test::TestRequest::get().uri("/hi").to_request();

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert("hi".into(), "https://linkedin.com/in/tsauvajon".into());

        let mut app = test::init_service(
            App::new()
                .data(web::Data::new(RwLock::new(db)))
                .service(browse),
        )
        .await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::FOUND);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(
            &Body::from("redirecting to https://linkedin.com/in/tsauvajon..."),
            body
        );

        assert_eq!(
            resp.headers().get("Location"),
            Some(&HeaderValue::from_str("https://linkedin.com/in/tsauvajon").unwrap())
        )
    }

    // try to follow a shortened URL that doesn't exist
    #[actix_rt::test]
    async fn test_link_miss() {
        let req = test::TestRequest::get()
            .uri("/thislinkdoesntexist")
            .to_request();

        let db: Db = web::Data::new(RwLock::new(HashMap::new()));

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
    async fn test_collision() {
        let req = test::TestRequest::post()
            .uri("/alreadyexists")
            .set_payload("https://something.new")
            .to_request();

        let mut db: HashMap<String, String> = HashMap::new();
        db.insert(
            "alreadyexists".into(),
            "https://github.com/tsauvajon".into(),
        );

        let mut app = test::init_service(
            App::new()
                .data(web::Data::new(RwLock::new(db)))
                .service(create_with_id),
        )
        .await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(&Body::from("already registered"), body);
    }
}
