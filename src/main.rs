#![feature(map_try_insert)]

use actix_web::{error, get, post, web, App, HttpServer, Responder};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use url::Url;

const MAX_SIZE: usize = 1_024; // max payload size is 1k

type Db = Arc<Mutex<HashMap<String, String>>>;

#[get("/{id}")]
async fn browse(db: web::Data<Db>, web::Path((id,)): web::Path<(String,)>) -> impl Responder {
    let db = db.lock().unwrap();

    match db.get(&id) {
        None => Err(error::ErrorNotFound("not found")),
        Some(url) => Ok(format!("redirecting to {}...", url)),
    }
}

#[post("/{id}")]
async fn create(
    db: web::Data<Db>,
    mut payload: web::Payload,
    web::Path((id,)): web::Path<(String,)>,
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

    let mut db = db.lock().unwrap();
    match db.try_insert(id.clone(), target.clone()) {
        Ok(_) => Ok(format!("/{} now redirects to {}", id, target)),
        Err(_) => Err(error::ErrorBadRequest("already registered")),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db: Db = Arc::new(Mutex::new(HashMap::new()));

    HttpServer::new(move || App::new().data(db.clone()).service(browse).service(create))
        .bind("127.0.0.1:8080")?
        .run()
        .await
}
