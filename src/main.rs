#![feature(map_try_insert)]

use actix_web::{error, get, post, web, App, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::RwLock;
use url::Url;

const MAX_SIZE: usize = 1_024; // max payload size is 1k

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

#[post("/{id}")]
async fn create(
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
    if let Err(err) = Url::parse(&target) {
        return Err(error::ErrorBadRequest(format!("malformed URL: {}", err)));
    };

    let mut db = db.write().unwrap();
    match db.try_insert(id.clone(), target.clone()) {
        Ok(_) => Ok(format!("/{} now redirects to {}", id, target)),
        Err(_) => Err(error::ErrorBadRequest("already registered")),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db: Db = web::Data::new(RwLock::new(HashMap::new()));

    HttpServer::new(move || App::new().data(db.clone()).service(browse).service(create))
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

    #[actix_rt::test]
    async fn test_test_create_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/hello")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = web::Data::new(RwLock::new(HashMap::new()));

        let mut app = test::init_service(App::new().data(db.clone()).service(create)).await;
        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let db = db.read().unwrap();
        assert_eq!(db.get("hello"), Some(&"https://hello.world".to_string()));
        assert_eq!(db.get("wwerwewrew"), None);
    }

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

    // querying a shortened URL that doesn't exist
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

    // trying to add a link for an already existing short-url
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
                .service(create),
        )
        .await;
        let mut resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.take_body();
        let body = body.as_ref().unwrap();
        assert_eq!(&Body::from("already registered"), body);
    }
}
