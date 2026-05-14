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

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::RwLock,
};

#[cfg(all(not(coverage), not(tarpaulin_include)))]
use actix_files::Files;
#[cfg(all(not(coverage), not(tarpaulin_include)))]
use actix_web::HttpServer;
use actix_web::{error, get, post, put, web, web::Data, App, HttpResponse, Responder};
use futures::StreamExt;
use structopt::StructOpt;
use url::Url;

const MAX_SIZE: usize = 256; // max payload size is 256 Kb
const RANDOM_URL_SIZE: usize = 5; // ramdomly generated URLs are 5 characters long

/// Error propagated when an `insert` fails to persist to disk.
///
/// The in-memory state is rolled back before the error is returned, so a
/// caller that surfaces this error can be sure the DB state on disk and
/// in memory remain consistent.
#[derive(Debug)]
struct PersistError(String);

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "persist: {}", self.0)
    }
}

struct Database {
    data: BTreeMap<String, String>,
    persistence: Option<PathBuf>,
}

trait PersistWriter {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
    fn sync_all(&mut self) -> io::Result<()>;
}

impl PersistWriter for std::fs::File {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        Write::write_all(self, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(self)
    }

    fn sync_all(&mut self) -> io::Result<()> {
        std::fs::File::sync_all(self)
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl Database {
    fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    fn insert(&mut self, key: &str, value: &str) -> Result<Option<String>, PersistError> {
        let previous_value = self.data.insert(key.to_string(), value.to_string());

        if let Some(path) = &self.persistence {
            if let Err(err) = persist_database(path, &self.data) {
                // Roll back the in-memory change so state stays consistent
                // with what's on disk.
                match &previous_value {
                    Some(old) => {
                        self.data.insert(key.to_string(), old.clone());
                    }
                    None => {
                        self.data.remove(key);
                    }
                }
                return Err(PersistError(err.to_string()));
            }
        }

        Ok(previous_value)
    }

    fn new(data: BTreeMap<String, String>) -> Self {
        Database {
            data,
            persistence: None,
        }
    }

    fn with_persistence(mut self, persistence: PathBuf) -> Self {
        self.persistence = Some(persistence);
        self
    }
}

#[test]
fn test_insert_data_returns_previous() {
    use std::env::temp_dir;

    let dir = temp_dir();
    let tmpfile_path = PathBuf::from(format!("{}/tmpfile2.txt", dir.to_str().unwrap()));

    let mut data = Database::new(BTreeMap::new()).with_persistence(tmpfile_path);
    let outcome = data.insert("hi", "qwerty").unwrap();
    assert_eq!(None, outcome);

    let outcome = data.insert("hi", "zxcvbnm").unwrap();
    assert_eq!(Some("qwerty".to_string()), outcome);
}

#[test]
fn test_insert_persists_updates() {
    use std::{env::temp_dir, fs};

    let tmp_dir = temp_dir();
    let tmpfile_path = PathBuf::from(format!(
        "{}/tmpfile-insert-update.txt",
        tmp_dir.to_str().unwrap()
    ));

    {
        let mut data = Database::new(BTreeMap::new()).with_persistence(tmpfile_path.clone());
        data.insert("foo", "bar").unwrap();
        data.insert("foo", "baz").unwrap();
    }

    let got = fs::read_to_string(&tmpfile_path).unwrap();
    assert_eq!("foo: baz\n".to_string(), got,);
}

#[test]
fn test_insert_atomic_no_tmp_left_after_success() {
    use std::env::temp_dir;

    let tmp_dir = temp_dir();
    let db_path = PathBuf::from(format!(
        "{}/atomic-no-tmp-left.yml",
        tmp_dir.to_str().unwrap()
    ));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(tmp_path_for(&db_path));

    let mut data = Database::new(BTreeMap::new()).with_persistence(db_path.clone());
    data.insert("a", "http://example.com/a").unwrap();
    data.insert("b", "http://example.com/b").unwrap();
    data.insert("a", "http://example.com/a2").unwrap();

    // The sibling temp file must not linger after a successful write.
    assert!(
        !tmp_path_for(&db_path).exists(),
        "temp file should be removed by rename"
    );

    // Sanity check the final file is the latest state.
    let got = std::fs::read_to_string(&db_path).unwrap();
    assert!(got.contains("a: http://example.com/a2"));
    assert!(got.contains("b: http://example.com/b"));
}

#[test]
fn test_persist_database_survives_pre_rename_crash() {
    use std::env::temp_dir;

    // Simulate a crash where the temp file was written but the rename
    // never happened: the original file must remain intact and parseable.
    let tmp_dir = temp_dir();
    let db_path = PathBuf::from(format!(
        "{}/atomic-partial-write.yml",
        tmp_dir.to_str().unwrap()
    ));
    let tmp_path = tmp_path_for(&db_path);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&tmp_path);

    // Seed the target file with a known-good state.
    std::fs::write(&db_path, "good: http://good\n").unwrap();

    // Simulate a partially-written temp file that was never renamed.
    std::fs::write(&tmp_path, "truncated-garbage").unwrap();
    assert!(tmp_path.exists());

    // Re-open the DB and verify it parses only the final file.
    let cli = Cli {
        front_dist_directory: None,
        addr: None,
        database: Some(db_path.to_str().unwrap().to_string()),
    };
    let db = cli.open_db().unwrap();
    let data = db.read().unwrap();
    assert_eq!(data.get("good"), Some(&"http://good".to_string()));

    // Cleanup so we don't leave state around for other tests.
    let _ = std::fs::remove_file(&tmp_path);
}

#[derive(Clone)]
struct Db {
    data: web::Data<RwLock<Database>>,
}

impl Default for Db {
    fn default() -> Self {
        Self::new(Database::default())
    }
}

impl Db {
    fn read(
        &self,
    ) -> Result<
        std::sync::RwLockReadGuard<'_, Database>,
        std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Database>>,
    > {
        self.data.read()
    }

    fn write(
        &self,
    ) -> Result<
        std::sync::RwLockWriteGuard<'_, Database>,
        std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Database>>,
    > {
        self.data.write()
    }

    fn new(data: Database) -> Self {
        Db {
            data: web::Data::new(RwLock::new(data)),
        }
    }
}

/// Persist the entire in-memory database to disk as YAML, atomically.
fn persist_database(path: &Path, data: &BTreeMap<String, String>) -> io::Result<()> {
    let tmp = tmp_path_for(path);
    let result = write_and_rename(&tmp, path, data);
    if result.is_err() {
        // Best-effort cleanup so we don't leave a stale temp file behind.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_payload(file: &mut impl PersistWriter, payload: &[u8]) -> io::Result<()> {
    file.write_all(payload)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// Write `data` as YAML to `tmp`, flush, fsync, then rename over `path`.
fn write_and_rename(tmp: &Path, path: &Path, data: &BTreeMap<String, String>) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(tmp)?;

    let payload = serde_yaml::to_string(data).expect("serializing string map should not fail");
    #[cfg(test)]
    let write_result = if tmp.file_name() == Some(OsStr::new("goto-force-write-error.tmp")) {
        Err(io::Error::other("forced write error"))
    } else {
        write_payload(&mut file, payload.as_bytes())
    };
    #[cfg(not(test))]
    let write_result = write_payload(&mut file, payload.as_bytes());
    write_result?;
    drop(file);

    fs::rename(tmp, path)
}

/// Compute the sibling temp-file path used during atomic rewrites.
fn tmp_path_for(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map_or_else(|| OsString::from("database"), OsStr::to_os_string);
    file_name.push(".tmp");
    path.with_file_name(file_name)
}

/// browse redirects to the long URL hidden behind a short URL, or returns a
/// 404 not found error if the short URL doesn't exist.
#[get("/{id}")]
async fn browse(db: web::Data<Db>, path: web::Path<(String,)>) -> impl Responder {
    let (id,) = path.into_inner();
    match db.read() {
        Ok(db) => match db.get(&id) {
            None => Err(error::ErrorNotFound("not found")),
            Some(url) => Ok(HttpResponse::Found()
                .append_header(("Location", url.to_string()))
                .body(format!("redirecting to {url} ..."))),
        },
        Err(err) => {
            println!("accessing the db: {err}");
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
    loop {
        let next_chunk = payload.next().await;
        let Some(chunk) = next_chunk else {
            break;
        };

        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => return Err(err.to_string()),
        };
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err("overflow".to_string());
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body[..].to_vec()).map_err(|err| format!("invalid request body: {err}"))
}

enum UpsertShortUrlCommand {
    CreateShortUrl { id: Option<String> },
    UpdateShortUrl { id: String },
}

/// Reason an upsert can fail.
///
/// Separated into `BadRequest` (map to 400) and `Persist` (map to 500) so
/// HTTP handlers can classify responses correctly without string-matching.
#[derive(Debug, PartialEq)]
enum UpsertError {
    BadRequest(String),
    Persist(String),
}

impl std::fmt::Display for UpsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpsertError::BadRequest(s) | UpsertError::Persist(s) => write!(f, "{s}"),
        }
    }
}

/// Create a short URL redirecting to a long URL.
///
/// If you pass an `id` a parameter, your short URL will be` /{id}`.
///
/// If you pass `None` instead, it will be `/{hash of the target URL}`.
///
/// You can also update an existing short URL by id. It will replace
/// the existing target URL at `/{id}`.
fn upsert_short_url(
    db: web::Data<Db>,
    target: &str,
    command: UpsertShortUrlCommand,
) -> Result<String, UpsertError> {
    if let Err(err) = Url::parse(target) {
        return Err(UpsertError::BadRequest(format!("malformed URL: {err}")));
    };

    let id = match &command {
        UpsertShortUrlCommand::CreateShortUrl { id: Some(id) }
        | UpsertShortUrlCommand::UpdateShortUrl { id } => id,
        UpsertShortUrlCommand::CreateShortUrl { id: None } => &hash(target),
    };

    let mut db = db.write().unwrap();
    let previous_target = db.get(id).cloned();
    if let Some(previous_target) = previous_target {
        match command {
            UpsertShortUrlCommand::CreateShortUrl { .. } => {
                Err(UpsertError::BadRequest("already registered".to_string()))
            }
            UpsertShortUrlCommand::UpdateShortUrl { .. } => {
                if let Err(err) = db.insert(id, target) {
                    return Err(UpsertError::Persist(err.to_string()));
                }
                Ok(format!(
                    "/{id} now redirects to {target} (was {previous_target})"
                ))
            }
        }
    } else {
        if let Err(err) = db.insert(id, target) {
            return Err(UpsertError::Persist(err.to_string()));
        }
        Ok(format!("/{id} now redirects to {target}"))
    }
}

impl From<UpsertError> for actix_web::Error {
    fn from(err: UpsertError) -> Self {
        match err {
            UpsertError::BadRequest(msg) => error::ErrorBadRequest(msg),
            UpsertError::Persist(msg) => error::ErrorInternalServerError(msg),
        }
    }
}

#[post("/{id}")]
async fn create_with_id(
    db: web::Data<Db>,
    payload: web::Payload,
    path: web::Path<(String,)>,
) -> impl Responder {
    let (id,) = path.into_inner();
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    let command = UpsertShortUrlCommand::CreateShortUrl { id: Some(id) };
    upsert_short_url(db, &target, command).map_err(actix_web::Error::from)
}

#[put("/{id}")]
async fn update_with_id(
    db: web::Data<Db>,
    payload: web::Payload,
    path: web::Path<(String,)>,
) -> impl Responder {
    let (id,) = path.into_inner();
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    let command = UpsertShortUrlCommand::UpdateShortUrl { id };
    upsert_short_url(db, &target, command).map_err(actix_web::Error::from)
}

#[post("/")]
async fn create_random(db: web::Data<Db>, payload: web::Payload) -> impl Responder {
    let target = match read_target(payload).await {
        Ok(target) => target,
        Err(err) => return Err(error::ErrorBadRequest(err)),
    };

    let command = UpsertShortUrlCommand::CreateShortUrl { id: None };
    upsert_short_url(db, &target, command).map_err(actix_web::Error::from)
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
        let Some(path_str) = &self.database else {
            return Ok(Db::default());
        };

        let path = Path::new(path_str);

        let mut file = match OpenOptions::new()
            .write(true)
            .create(true)
            .read(true)
            .truncate(false)
            .open(path)
        {
            Ok(file) => file,
            Err(err) => return Err(err.to_string()),
        };

        let mut buf = String::new();
        let Ok(len) = file.read_to_string(&mut buf) else {
            return Ok(Db::default());
        };

        let database = if len == 0 {
            Database::new(BTreeMap::new())
        } else {
            let yaml_contents: BTreeMap<String, String> =
                serde_yaml::from_str(&buf).map_err(|err| format!("parse data: {err}"))?;

            Database::new(yaml_contents)
        };

        drop(file);
        Ok(Db::new(database.with_persistence(path.to_path_buf())))
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

        assert!(data.persistence.is_none());
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

        assert!(matches!(
            &data.persistence,
            Some(path)
                if path.metadata().unwrap().is_file()
        ));
    }

    #[test]
    fn test_open_db_existing_file() {
        use std::{env::temp_dir, fs::File};

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

        assert!(data.persistence.is_some());
    }

    #[test]
    fn test_open_db_existing_file_invalid_utf8() {
        use std::{env::temp_dir, fs::File, io::Write};

        let dir = temp_dir();
        let tmpfile_path = format!("{}/tmpfile-invalid-utf8.txt", dir.to_str().unwrap());

        let mut file = File::create(&tmpfile_path).unwrap();
        file.write_all(&[0xff, 0xfe, 0xfd]).unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path),
        };
        let db = cli.open_db().unwrap();
        let data = db.read().unwrap();

        assert!(data.persistence.is_none());
        assert!(data.data.is_empty());
    }

    #[test]
    fn test_open_db_existing_file_with_data() {
        use std::{env::temp_dir, fs::File, io::Write};

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

        assert!(data.persistence.is_some());
        assert_eq!(data.data.get("hello"), Some(&"http://world".to_string()));
    }

    #[test]
    fn test_open_db_existing_file_with_bad_data() {
        use std::{env::temp_dir, fs::File, io::Write};

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
        assert!(matches!(res, Err(err) if err.contains("parse data: invalid type:")));
    }
}

#[actix_web::main]
#[cfg(not(coverage))]
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
            .app_data(Data::new(db.clone()))
            .service(browse)
            .service(create_random)
            .service(create_with_id)
            .service(update_with_id)
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
    fn test_persist_error_display() {
        assert_eq!(
            "persist: boom",
            PersistError("boom".to_string()).to_string()
        );
    }

    #[test]
    fn test_upsert_error_display() {
        assert_eq!(
            "bad",
            UpsertError::BadRequest("bad".to_string()).to_string()
        );
        assert_eq!(
            "persist failed",
            UpsertError::Persist("persist failed".to_string()).to_string()
        );
    }

    #[test]
    fn test_upsert_error_into_actix_error() {
        let bad_request = actix_web::Error::from(UpsertError::BadRequest("bad".to_string()));
        assert_eq!(
            actix_web::http::StatusCode::BAD_REQUEST,
            bad_request.as_response_error().status_code()
        );

        let persist = actix_web::Error::from(UpsertError::Persist("persist".to_string()));
        assert_eq!(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            persist.as_response_error().status_code()
        );
    }

    #[test]
    fn test_insert_rollback_restores_previous_value_on_persist_failure() {
        use std::env::temp_dir;

        let base_dir = temp_dir().join(format!(
            "goto-insert-rollback-existing-{}",
            std::process::id()
        ));
        let persistence_path = base_dir.join("missing-parent").join("db.yml");

        let mut database = Database::new(BTreeMap::from([(
            "foo".to_string(),
            "https://old.example".to_string(),
        )]))
        .with_persistence(persistence_path);

        let err = database.insert("foo", "https://new.example").unwrap_err();
        assert!(err.to_string().contains("persist:"));
        assert_eq!(
            Some(&"https://old.example".to_string()),
            database.get("foo")
        );
    }

    #[test]
    fn test_insert_rollback_removes_new_key_on_persist_failure() {
        use std::env::temp_dir;

        let base_dir = temp_dir().join(format!("goto-insert-rollback-new-{}", std::process::id()));
        let persistence_path = base_dir.join("missing-parent").join("db.yml");

        let mut database = Database::new(BTreeMap::new()).with_persistence(persistence_path);

        let err = database.insert("foo", "https://new.example").unwrap_err();
        assert!(err.to_string().contains("persist:"));
        assert_eq!(None, database.get("foo"));
    }

    #[test]
    fn test_create_short_malformed_url() {
        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let target = "this is not a valid URL".to_string();
        let command = UpsertShortUrlCommand::CreateShortUrl {
            id: Some("hello".to_string()),
        };
        assert_eq!(
            Err(UpsertError::BadRequest(
                "malformed URL: relative URL without a base".to_string()
            )),
            upsert_short_url(web::Data::new(db), &target, command)
        );
    }

    #[test]
    fn test_create_short_url() {
        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let target = "https://google.com".to_string();
        let id = "hello";
        let command = UpsertShortUrlCommand::CreateShortUrl {
            id: Some(id.to_string()),
        };
        upsert_short_url(web::Data::new(db.clone()), &target, command).unwrap();

        let db = db.read().unwrap();
        let got = db.get(id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_hashed_id() {
        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let target = "https://google.com";
        let command = UpsertShortUrlCommand::CreateShortUrl { id: None };
        upsert_short_url(web::Data::new(db.clone()), target, command).unwrap();

        let id = hash(target);
        let db = db.read().unwrap();
        let got = db.get(&id).unwrap();
        assert_eq!(&target, got);
    }

    #[test]
    fn test_create_short_url_already_exists() {
        let id = "hello";

        let mut db: BTreeMap<String, String> = BTreeMap::new();
        db.insert(id.into(), "some existing value".into());
        let db: Db = Db::new(Database::new(db));

        let target = "https://google.com";
        let command = UpsertShortUrlCommand::CreateShortUrl {
            id: Some(id.to_string()),
        };
        assert_eq!(
            Err(UpsertError::BadRequest("already registered".to_string())),
            upsert_short_url(web::Data::new(db), target, command)
        );
    }

    #[test]
    fn test_update_existing_url() {
        let id = "hello";
        let mut db: BTreeMap<String, String> = BTreeMap::new();
        db.insert(id.into(), "https://google.com".into());
        let db: Db = Db::new(Database::new(db));

        // Replace with hello -> yahoo.com
        let target = "https://yahoo.com";
        let command = UpsertShortUrlCommand::UpdateShortUrl { id: id.to_string() };
        let result = upsert_short_url(Data::new(db), target, command);
        assert_eq!(
            result,
            Ok("/hello now redirects to https://yahoo.com (was https://google.com)".to_string())
        )
    }

    #[test]
    fn test_upsert_short_url_returns_persist_error_when_updating_existing_key() {
        use std::env::temp_dir;

        let path = temp_dir()
            .join(format!("goto-upsert-existing-{}", std::process::id()))
            .join("missing")
            .join("db.yml");
        let db = Db::new(
            Database::new(BTreeMap::from([(
                "hello".to_string(),
                "https://google.com".to_string(),
            )]))
            .with_persistence(path),
        );

        let command = UpsertShortUrlCommand::UpdateShortUrl {
            id: "hello".to_string(),
        };
        let result = upsert_short_url(Data::new(db), "https://yahoo.com", command).unwrap_err();

        let err = actix_web::Error::from(result);
        assert_eq!(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.as_response_error().status_code()
        );
    }

    #[test]
    fn test_update_url_that_does_not_exist() {
        let id = "hello";
        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let target = "https://google.com";
        let command = UpsertShortUrlCommand::UpdateShortUrl { id: id.to_string() };
        assert_eq!(
            Ok("/hello now redirects to https://google.com".to_string()),
            upsert_short_url(web::Data::new(db), target, command)
        );
    }

    #[test]
    fn test_upsert_short_url_returns_persist_error_when_creating_new_key() {
        use std::env::temp_dir;

        let path = temp_dir()
            .join(format!("goto-upsert-new-{}", std::process::id()))
            .join("missing")
            .join("db.yml");
        let db = Db::new(Database::new(BTreeMap::new()).with_persistence(path));

        let command = UpsertShortUrlCommand::UpdateShortUrl {
            id: "hello".to_string(),
        };
        let result = upsert_short_url(Data::new(db), "https://google.com", command).unwrap_err();

        let err = actix_web::Error::from(result);
        assert_eq!(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.as_response_error().status_code()
        );
    }

    #[test]
    fn test_read_database() {
        let data = "hello: http://hello-world.com\nkey2: value2";

        let yaml_contents: BTreeMap<String, String> = serde_yaml::from_str(data).unwrap();
        println!("{:?}", yaml_contents);
    }

    #[test]
    fn test_write_database() {
        use std::env::temp_dir;

        let mut database: BTreeMap<String, String> = BTreeMap::new();
        database.insert(
            "tsauvajon".to_string(),
            "https://linkedin.com/in/tsauvajon".to_string(),
        );
        let want = serde_yaml::to_string(&database).unwrap();

        let tmp_dir = temp_dir();
        let tmpfile_path = PathBuf::from(format!(
            "{}/persist_database.yml",
            tmp_dir.to_str().unwrap()
        ));
        let _ = std::fs::remove_file(&tmpfile_path);

        persist_database(&tmpfile_path, &database).unwrap();

        let got = std::fs::read_to_string(&tmpfile_path).unwrap();

        assert_eq!(want, got);
    }

    #[test]
    fn test_persist_database_writes_keys_alphabetically() {
        use std::env::temp_dir;

        let database = BTreeMap::from([
            ("zebra".to_string(), "last".to_string()),
            ("apple".to_string(), "first".to_string()),
            ("mango".to_string(), "middle".to_string()),
        ]);
        let tmpfile_path =
            temp_dir().join(format!("goto-persist-sorted-{}.yml", std::process::id()));
        let _ = std::fs::remove_file(&tmpfile_path);

        persist_database(&tmpfile_path, &database).unwrap();

        let got = std::fs::read_to_string(&tmpfile_path).unwrap();
        assert_eq!("apple: first\nmango: middle\nzebra: last\n", got);

        let _ = std::fs::remove_file(&tmpfile_path);
    }

    #[test]
    fn test_open_db_rewrites_existing_database_alphabetically() {
        use std::env::temp_dir;

        let tmpfile_path = temp_dir().join(format!(
            "goto-existing-db-sorted-{}.yml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmpfile_path);
        std::fs::write(&tmpfile_path, "zebra: last\napple: first\nmango: middle\n").unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(tmpfile_path.to_str().unwrap().to_string()),
        };

        let db = cli.open_db().unwrap();
        let mut data = db.write().unwrap();
        data.insert("banana", "second").unwrap();
        drop(data);

        let got = std::fs::read_to_string(&tmpfile_path).unwrap();
        assert_eq!(
            "apple: first\nbanana: second\nmango: middle\nzebra: last\n",
            got
        );

        let _ = std::fs::remove_file(&tmpfile_path);
    }

    struct MockPersistWriter {
        fail_on_write: bool,
        fail_on_flush: bool,
        fail_on_sync: bool,
        bytes: Vec<u8>,
    }

    impl PersistWriter for MockPersistWriter {
        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            if self.fail_on_write {
                return Err(io::Error::other("write failed"));
            }

            self.bytes.extend_from_slice(buf);
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_on_flush {
                return Err(io::Error::other("flush failed"));
            }

            Ok(())
        }

        fn sync_all(&mut self) -> io::Result<()> {
            if self.fail_on_sync {
                return Err(io::Error::other("sync failed"));
            }

            Ok(())
        }
    }

    #[test]
    fn test_write_payload() {
        let mut writer = MockPersistWriter {
            fail_on_write: false,
            fail_on_flush: false,
            fail_on_sync: false,
            bytes: Vec::new(),
        };

        write_payload(&mut writer, b"hello").unwrap();
        assert_eq!(b"hello", writer.bytes.as_slice());
    }

    #[test]
    fn test_write_payload_write_error() {
        let mut writer = MockPersistWriter {
            fail_on_write: true,
            fail_on_flush: false,
            fail_on_sync: false,
            bytes: Vec::new(),
        };

        let err = write_payload(&mut writer, b"hello").unwrap_err();
        assert_eq!(io::ErrorKind::Other, err.kind());
    }

    #[test]
    fn test_write_payload_flush_error() {
        let mut writer = MockPersistWriter {
            fail_on_write: false,
            fail_on_flush: true,
            fail_on_sync: false,
            bytes: Vec::new(),
        };

        let err = write_payload(&mut writer, b"hello").unwrap_err();
        assert_eq!(io::ErrorKind::Other, err.kind());
    }

    #[test]
    fn test_write_payload_sync_error() {
        let mut writer = MockPersistWriter {
            fail_on_write: false,
            fail_on_flush: false,
            fail_on_sync: true,
            bytes: Vec::new(),
        };

        let err = write_payload(&mut writer, b"hello").unwrap_err();
        assert_eq!(io::ErrorKind::Other, err.kind());
    }

    #[test]
    fn test_write_and_rename_propagates_write_error() {
        use std::env::temp_dir;

        let tmp = temp_dir().join("goto-force-write-error.tmp");
        let path = temp_dir().join(format!("goto-write-and-rename-out-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&path);

        let data = BTreeMap::from([("foo".to_string(), "bar".to_string())]);
        let err = write_and_rename(&tmp, &path, &data).unwrap_err();

        assert_eq!(io::ErrorKind::Other, err.kind());
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persist_database_removes_tmp_on_rename_failure() {
        use std::env::temp_dir;

        let path = temp_dir().join(format!("goto-persist-dir-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();

        let tmp_path = tmp_path_for(&path);
        let _ = std::fs::remove_file(&tmp_path);

        let data = BTreeMap::from([("foo".to_string(), "bar".to_string())]);
        let result = persist_database(&path, &data);

        assert!(result.is_err());
        assert!(
            !tmp_path.exists(),
            "tmp file should be cleaned up on failure"
        );

        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_tmp_path_for_path_without_filename() {
        let path = Path::new("/");
        assert_eq!(PathBuf::from("/database.tmp"), tmp_path_for(path));
    }

    #[test]
    fn test_open_db_open_error() {
        use std::env::temp_dir;

        let path = temp_dir().join(format!("goto-open-db-dir-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();

        let cli = Cli {
            front_dist_directory: None,
            addr: None,
            database: Some(path.to_str().unwrap().to_string()),
        };

        let result = cli.open_db();
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&path);
    }

    #[actix_rt::test]
    async fn test_read_target_payload_error() {
        use std::pin::Pin;

        use actix_web::{error::PayloadError, FromRequest};

        let stream = futures::stream::once(async {
            Err::<web::Bytes, PayloadError>(PayloadError::EncodingCorrupted)
        });
        let mut payload = actix_web::dev::Payload::from(Box::pin(stream)
            as Pin<Box<dyn futures::Stream<Item = Result<web::Bytes, PayloadError>>>>);
        let req = actix_web::test::TestRequest::default().to_http_request();
        let payload = web::Payload::from_request(&req, &mut payload)
            .await
            .unwrap();

        let result = read_target(payload).await;
        assert_eq!(Err("can not decode content-encoding".to_string()), result);
    }
}

#[cfg(test)]
mod integration_tests {
    use actix_web::{
        body::MessageBody,
        http::{header::HeaderValue, StatusCode},
        test,
    };

    use super::*;

    // create a new custom shorturl
    #[actix_rt::test]
    async fn integration_test_create_custom_shortened_url() {
        let req = test::TestRequest::post()
            .uri("/hello")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app = test::init_service(
            App::new()
                .app_data(Data::new(db.clone()))
                .service(create_with_id),
        )
        .await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let db = db.read().unwrap();
        assert_eq!(db.get("hello"), Some(&"https://hello.world".to_string()));
        assert_eq!(db.get("wwerwewrew"), None);
    }

    // update an existing custom shorturl
    #[actix_rt::test]
    async fn integration_test_update_shortened_url() {
        let req = test::TestRequest::put()
            .uri("/hello")
            .set_payload("https://hello.world")
            .to_request();

        let db: Db = Db::new(Database::new(BTreeMap::from([(
            "hello".to_string(),
            "https://google.com".to_string(),
        )])));

        let app = test::init_service(
            App::new()
                .app_data(Data::new(db.clone()))
                .service(update_with_id),
        )
        .await;
        let resp = test::call_service(&app, req).await;
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

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app = test::init_service(
            App::new()
                .app_data(Data::new(db.clone()))
                .service(create_random),
        )
        .await;
        let resp = test::call_service(&app, req).await;
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

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app = test::init_service(
            App::new()
                .app_data(Data::new(db.clone()))
                .service(create_random),
        )
        .await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!(
            "invalid request body: invalid utf-8 sequence of 1 bytes from index 1",
            body
        );
    }

    #[actix_rt::test]
    async fn integration_test_create_random_shortened_url_overflow() {
        let req = test::TestRequest::post()
            .uri("/toolong")
            .set_payload(vec![b'a'; 2000])
            .to_request();

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app = test::init_service(
            App::new()
                .app_data(Data::new(db.clone()))
                .service(create_with_id),
        )
        .await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!("overflow", body);
    }

    #[actix_rt::test]
    async fn integration_test_update_shortened_url_bad_body() {
        let req = test::TestRequest::put()
            .uri("/hello")
            .set_payload(vec![0, 159, 146, 150])
            .to_request();

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app =
            test::init_service(App::new().app_data(Data::new(db)).service(update_with_id)).await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!(
            "invalid request body: invalid utf-8 sequence of 1 bytes from index 1",
            body
        );
    }

    // follow an existing shorturl
    #[actix_rt::test]
    async fn integration_test_use_shortened_url() {
        let req = test::TestRequest::get().uri("/hi").to_request();

        let mut db: BTreeMap<String, String> = BTreeMap::new();
        db.insert("hi".into(), "https://linkedin.com/in/tsauvajon".into());

        let db: Db = Db::new(Database::new(db));

        let app = test::init_service(App::new().app_data(Data::new(db)).service(browse)).await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FOUND);

        assert_eq!(
            resp.headers().get("Location"),
            Some(&HeaderValue::from_str("https://linkedin.com/in/tsauvajon").unwrap())
        );

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!("redirecting to https://linkedin.com/in/tsauvajon ...", body);
    }

    #[actix_rt::test]
    async fn integration_test_poisoned_mutex() {
        use std::panic;

        let req = test::TestRequest::get().uri("/hi").to_request();
        let mut db: BTreeMap<String, String> = BTreeMap::new();
        db.insert("hi".into(), "https://linkedin.com/in/tsauvajon".into());
        let db: Db = Db::new(Database::new(db));

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

        let app = test::init_service(App::new().app_data(Data::new(db)).service(browse)).await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!("poisoned lock: another task failed inside", body);
    }

    // try to follow a shortened URL that doesn't exist
    #[actix_rt::test]
    async fn integration_test_link_miss() {
        let req = test::TestRequest::get()
            .uri("/thislinkdoesntexist")
            .to_request();

        let db: Db = Db::new(Database::new(BTreeMap::new()));

        let app = test::init_service(App::new().app_data(Data::new(db)).service(browse)).await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        assert_eq!(resp.headers().get("Location"), None);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!("not found", body);
    }

    // try to add a link for an already existing short-url
    #[actix_rt::test]
    async fn integration_test_collision() {
        let req = test::TestRequest::post()
            .uri("/alreadyexists")
            .set_payload("https://something.new")
            .to_request();

        let mut db: BTreeMap<String, String> = BTreeMap::new();
        db.insert(
            "alreadyexists".into(),
            "https://github.com/tsauvajon".into(),
        );

        let db: Db = Db::new(Database::new(db));
        let app =
            test::init_service(App::new().app_data(Data::new(db)).service(create_with_id)).await;
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.into_body().try_into_bytes().unwrap();
        assert_eq!("already registered", body);
    }
}
