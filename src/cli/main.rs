use async_trait::async_trait;
use home::home_dir;
use hyper::{Body, Method, Request};
use hyper::{Client as HyperClient, Uri};
use serde::{Deserialize, Serialize};
use std::convert::identity;
use std::fmt::Debug;
use std::fs::OpenOptions;
use std::path::PathBuf;
use structopt::StructOpt;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

#[derive(StructOpt, Clone)]
#[structopt(about = "Create shortened URLs")]
struct Args {
    #[structopt(help = "Shortened URL")]
    shorturl: String,
    #[structopt(help = "URL to shorten")]
    target: Option<String>,

    #[structopt(
        short = "f",
        long = "force",
        help = "Create the short URL, or if it already exists, update it instead"
    )]
    force_replace: bool,

    #[structopt(long = "api", help = "Base URL of the Goto API")]
    api_url: Option<String>,

    #[structopt(short = "s", long = "silent", help = "Don't print redirections")]
    silent: bool,

    #[structopt(short = "n", long = "no-open-browser", help = "Don't open the browser")]
    no_browser: bool,
}

#[derive(Debug, PartialEq)]
enum GoToError {
    NoRedirection,
    CliError(String),
    ApiError(String),
}

impl From<actix_web::http::uri::InvalidUri> for GoToError {
    fn from(error: actix_web::http::uri::InvalidUri) -> Self {
        GoToError::CliError(error.to_string())
    }
}

impl From<std::string::FromUtf8Error> for GoToError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        GoToError::ApiError(format!("expected utf8: {error}"))
    }
}

impl From<hyper::header::ToStrError> for GoToError {
    fn from(error: hyper::header::ToStrError) -> Self {
        GoToError::ApiError(error.to_string())
    }
}

struct CliOptions {
    shorturl: String,
    target: Option<String>,

    always_replace: bool,
    verbose: bool,
    open_browser: bool,
}

impl CliOptions {
    fn new(args: &Args, config: &Config) -> CliOptions {
        let always_replace = args.force_replace || config.force_replace.is_some_and(identity);
        let silent = args.silent || config.silent.is_some_and(identity);
        let no_browser = args.no_browser || config.no_browser.is_some_and(identity);

        CliOptions {
            shorturl: args.shorturl.to_owned(),
            target: args.target.to_owned(),
            always_replace,
            verbose: !silent,
            open_browser: !no_browser,
        }
    }
}

#[cfg(test)]
mod test_cli_options {
    use super::*;

    #[test]
    fn test_open_browser() {
        let mut args = Args {
            shorturl: String::new(),
            target: None,
            api_url: None,
            force_replace: false,
            silent: false,
            no_browser: false,
        };

        let mut config = Config {
            api_url: None,
            force_replace: None,
            silent: None,
            no_browser: None,
        };

        // default
        args.no_browser = false;
        config.no_browser = None;
        let got = CliOptions::new(&args, &config);
        assert!(got.open_browser);

        // both args and config agree
        args.no_browser = true;
        config.no_browser = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(!got.open_browser);

        args.no_browser = false;
        config.no_browser = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(got.open_browser);

        // args take precendence over config
        args.no_browser = true;
        config.no_browser = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(!got.open_browser);

        // only args
        args.no_browser = true;
        config.no_browser = None;
        let got = CliOptions::new(&args, &config);
        assert!(!got.open_browser);

        // only config
        args.no_browser = false;
        config.no_browser = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(!got.open_browser);
    }

    #[test]
    fn test_verbose() {
        let mut args = Args {
            shorturl: String::new(),
            target: None,
            api_url: None,
            force_replace: false,
            silent: false,
            no_browser: false,
        };

        let mut config = Config {
            api_url: None,
            force_replace: None,
            silent: None,
            no_browser: None,
        };

        // default
        args.silent = false;
        config.silent = None;
        let got = CliOptions::new(&args, &config);
        assert!(got.verbose);

        // both args and config agree
        args.silent = true;
        config.silent = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(!got.verbose);

        args.silent = false;
        config.silent = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(got.verbose);

        // args take precendence over config
        args.silent = true;
        config.silent = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(!got.verbose);

        // only args
        args.silent = true;
        config.silent = None;
        let got = CliOptions::new(&args, &config);
        assert!(!got.verbose);

        // only config
        args.silent = false;
        config.silent = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(!got.verbose);
    }

    #[test]
    fn test_force() {
        let mut args = Args {
            shorturl: String::new(),
            target: None,
            api_url: None,
            force_replace: false,
            silent: false,
            no_browser: false,
        };

        let mut config = Config {
            api_url: None,
            force_replace: None,
            silent: None,
            no_browser: None,
        };

        // default
        args.force_replace = false;
        config.force_replace = None;
        let got = CliOptions::new(&args, &config);
        assert!(!got.always_replace);

        // both args and config agree
        args.force_replace = true;
        config.force_replace = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(got.always_replace);

        args.force_replace = false;
        config.force_replace = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(!got.always_replace);

        // args take precendence over config
        args.force_replace = true;
        config.force_replace = Some(false);
        let got = CliOptions::new(&args, &config);
        assert!(got.always_replace);

        // only args
        args.force_replace = true;
        config.force_replace = None;
        let got = CliOptions::new(&args, &config);
        assert!(got.always_replace);

        // only config
        args.force_replace = false;
        config.force_replace = Some(true);
        let got = CliOptions::new(&args, &config);
        assert!(got.always_replace);
    }
}

struct Cli<C: Client> {
    options: CliOptions,
    client: C,
}

impl<C: Client> Cli<C> {
    async fn run(self) -> Result<(), GoToError> {
        match self.options.target {
            Some(target) => {
                if self.options.always_replace {
                    self.client.update_url(self.options.shorturl, target).await
                } else {
                    self.client.create_new(self.options.shorturl, target).await
                }
            }
            None => {
                let location = self.client.get_long_url(self.options.shorturl).await?;

                display_location(&location, self.options.verbose, &mut std::io::stdout());
                open_location(&location, self.options.open_browser);

                Ok(())
            }
        }
    }
}

fn display_location(loc: &str, verbose: bool, mut writer: impl std::io::Write) {
    if verbose {
        writeln!(writer, "redirecting to {loc}").unwrap();
    }
}

#[test]
fn test_display_location_silent() {
    let mut result = Vec::new();
    display_location("hi there", false, &mut result);

    assert_eq!(b"".to_vec(), result);
}

#[test]
fn test_display_location_verbose() {
    let mut result = Vec::new();
    display_location("http://hi.there", true, &mut result);

    assert_eq!(b"redirecting to http://hi.there\n".to_vec(), result,);
}

#[cfg(not(tarpaulin_include))]
fn open_location(loc: &str, browser: bool) {
    if browser {
        webbrowser::open(loc).unwrap();
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Config {
    api_url: Option<String>,
    force_replace: Option<bool>,
    silent: Option<bool>,
    no_browser: Option<bool>,
}

fn open_or_create_config(filepath: &PathBuf) -> Result<Config, GoToError> {
    let _ = std::fs::create_dir_all(filepath.parent().unwrap());

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .read(true)
        .truncate(false)
        .open(filepath)
        .map_err(|err| GoToError::CliError(format!("open config file: {err}")))?;

    read_or_write_config(file)
}

fn read_or_write_config(
    mut file: impl std::io::Read + std::io::Write,
) -> Result<Config, GoToError> {
    let mut buf = String::new();
    match file.read_to_string(&mut buf) {
        Err(err) => Err(GoToError::CliError(format!("read config file: {err}"))),
        Ok(len) => {
            if len == 0 {
                let default = Config {
                    silent: Some(false),
                    force_replace: Some(false),
                    no_browser: Some(false),
                    api_url: Some(DEFAULT_API_URL.to_string()),
                };

                file.write_all(serde_yaml::to_string(&default).unwrap().as_bytes())
                    .map_err(|err| GoToError::CliError(format!("write default config: {err}",)))?;

                Ok(default)
            } else {
                let yaml_contents = serde_yaml::from_str(&buf)
                    .map_err(|err| GoToError::CliError(format!("parse config data: {err}")))?;

                Ok(yaml_contents)
            }
        }
    }
}

#[cfg(test)]
mod config_tests {
    use std::env::temp_dir;
    use std::fs::File;
    use std::io::{Cursor, Error, Read, Result, Write};

    use super::*;

    #[test]
    fn test_create_config_when_missing() {
        let mut data: Vec<u8> = Vec::new();
        let mut mock_file = Cursor::new(&mut data);

        read_or_write_config(&mut mock_file).unwrap();

        let got = String::from_utf8(data).unwrap();
        assert!(got.contains("silent: false"), "{}", got);
        assert!(got.contains("no_browser: false"), "{}", got);
        assert!(got.contains("api_url: \"http://"), "{}", got);
    }

    #[test]
    fn test_read_existing_config() {
        let mut data: Vec<u8> = Vec::from("silent: true\napi_url: \"hello\"");
        let mut mock_file = Cursor::new(&mut data);

        let got = read_or_write_config(&mut mock_file).unwrap();

        assert_eq!(Some(true), got.silent);
        assert_eq!(None, got.no_browser);
        assert_eq!(Some("hello".to_string()), got.api_url);
        assert_eq!(
            "silent: true\napi_url: \"hello\"".to_string(),
            String::from_utf8(data).unwrap()
        );
    }

    #[test]
    fn test_create_config() {
        let mut filepath = temp_dir();
        filepath.push("test_create_config.yml");

        let got = open_or_create_config(&filepath);
        assert!(got.is_ok());

        let mut file = File::open(&filepath).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();

        assert!(content.contains("api_url"));
    }

    #[test]
    fn test_open_config() {
        let mut filepath = temp_dir();
        filepath.push("test_existing_config.yml");

        let mut file = File::create(&filepath).unwrap();
        file.write_all(b"api_url: \"http://hello.world\"\n")
            .unwrap();

        let got = open_or_create_config(&filepath).unwrap();
        assert_eq!(Some("http://hello.world".to_string()), got.api_url);
    }

    #[test]
    fn test_open_config_invalid_data() {
        let mut filepath = temp_dir();
        filepath.push("contains_invalid_data");

        let mut file = File::create(&filepath).unwrap();
        file.write_all(b"what is this... it doesn't look like valid YAML!{ }}}} P{{")
            .unwrap();

        let got = open_or_create_config(&filepath);
        assert!(got.is_err());

        let err = got.err().unwrap();
        assert!(
            format!("{:?}", err).contains("parse config data:"),
            "{:?}",
            err
        );
    }

    #[test]
    fn test_open_config_wrong_file() {
        let mut filepath = temp_dir();
        filepath.push("{}///////\\\\\\////");

        let got = open_or_create_config(&filepath);
        assert!(got.is_err());

        let err = got.err().unwrap();
        assert!(
            format!("{:?}", err).contains("open config file:"),
            "{:?}",
            err
        );
    }

    struct RWMockCantRead {}

    impl std::io::Read for RWMockCantRead {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
            Err(Error::other("oh no!"))
        }
    }

    impl std::io::Write for RWMockCantRead {
        fn write(&mut self, _buf: &[u8]) -> Result<usize> {
            todo!()
        }

        fn flush(&mut self) -> Result<()> {
            todo!()
        }
    }

    #[test]
    fn test_cannot_read_config() {
        let mut mock_file = RWMockCantRead {};

        let got = read_or_write_config(&mut mock_file);
        let want = Err(GoToError::CliError("read config file: oh no!".to_string()));
        assert_eq!(want, got);
    }

    struct RWMockCantWrite {}

    impl std::io::Read for RWMockCantWrite {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
            Ok(0)
        }
    }

    impl std::io::Write for RWMockCantWrite {
        fn write(&mut self, _buf: &[u8]) -> Result<usize> {
            Err(Error::other("that went terribly wrong!"))
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_cannot_write_config() {
        let mut mock_file = RWMockCantWrite {};

        let got = read_or_write_config(&mut mock_file);
        let want = Err(GoToError::CliError(
            "write default config: that went terribly wrong!".to_string(),
        ));
        assert_eq!(want, got);
    }
}

#[cfg(test)]
mod cant_read_config_tests {}

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<(), GoToError> {
    let args = Args::from_args();

    let mut filepath = home_dir().unwrap();
    filepath.push(".goto");
    filepath.push("config.yml");

    let config = open_or_create_config(&filepath).unwrap();

    let options = CliOptions::new(&args, &config);
    let api_url = get_api_url(&args, &config);

    let cli = Cli {
        options,
        client: HttpClient::new(api_url),
    };

    cli.run().await
}

fn get_api_url(args: &Args, config: &Config) -> String {
    match &args.api_url {
        Some(api_url) => api_url.to_owned(),
        None => match config.api_url.to_owned() {
            Some(api_url) => api_url,
            None => DEFAULT_API_URL.to_string(),
        },
    }
}

#[test]
fn test_get_api_url() {
    let mut args = Args {
        shorturl: String::new(),
        target: None,
        api_url: None,
        force_replace: false,
        silent: false,
        no_browser: false,
    };

    let mut config = Config {
        api_url: None,
        force_replace: None,
        silent: None,
        no_browser: None,
    };

    // default
    args.api_url = None;
    config.api_url = None;
    let got = get_api_url(&args, &config);
    assert_eq!(DEFAULT_API_URL, got);

    // both args and config agree
    args.api_url = Some("a".to_string());
    config.api_url = Some("a".to_string());
    let got = get_api_url(&args, &config);
    assert_eq!("a".to_string(), got);

    // args take precendence over config
    args.api_url = Some("a".to_string());
    config.api_url = Some("b".to_string());
    let got = get_api_url(&args, &config);
    assert_eq!("a".to_string(), got);

    // only args
    args.api_url = Some("a".to_string());
    config.api_url = None;
    let got = get_api_url(&args, &config);
    assert_eq!("a".to_string(), got);

    // only config
    args.api_url = None;
    config.api_url = Some("a".to_string());
    let got = get_api_url(&args, &config);
    assert_eq!("a".to_string(), got);
}

#[async_trait]
trait Client {
    async fn create_new(self, shorturl: String, target: String) -> Result<(), GoToError>;

    async fn update_url(self, shorturl: String, target: String) -> Result<(), GoToError>;

    async fn get_long_url(self, shorturl: String) -> Result<String, GoToError>;
}

#[cfg(test)]
mod cli_test {
    use super::*;

    struct MockClient {
        create_new_called_with: Option<(String, String)>,
        want_create_new_called_with: Option<(String, String)>,

        update_url_called_with: Option<(String, String)>,
        want_update_url_called_with: Option<(String, String)>,

        get_long_url_called_with: Option<String>,
        want_get_long_url_called_with: Option<String>,
    }

    impl MockClient {
        fn new() -> Self {
            MockClient {
                create_new_called_with: None,
                want_create_new_called_with: None,

                update_url_called_with: None,
                want_update_url_called_with: None,

                get_long_url_called_with: None,
                want_get_long_url_called_with: None,
            }
        }
    }

    #[async_trait]
    impl Client for MockClient {
        async fn create_new(mut self, shorturl: String, target: String) -> Result<(), GoToError> {
            self.create_new_called_with = Some((shorturl, target));
            Ok(())
        }

        async fn update_url(mut self, shorturl: String, target: String) -> Result<(), GoToError> {
            self.update_url_called_with = Some((shorturl, target));
            Ok(())
        }

        async fn get_long_url(mut self, shorturl: String) -> Result<String, GoToError> {
            self.get_long_url_called_with = Some(shorturl);
            Ok(String::new())
        }
    }

    impl Drop for MockClient {
        fn drop(&mut self) {
            let want = self.want_create_new_called_with.as_ref();
            let got = self.create_new_called_with.as_ref();
            assert_eq!(want, got);

            let want = self.want_update_url_called_with.as_ref();
            let got = self.update_url_called_with.as_ref();
            assert_eq!(want, got);

            let want = self.want_get_long_url_called_with.as_ref();
            let got = self.get_long_url_called_with.as_ref();
            assert_eq!(want, got);
        }
    }

    #[actix_rt::test]
    async fn test_cli_create_new() {
        let mut client = MockClient::new();
        client.want_create_new_called_with =
            Some(("hello".to_string(), "http://world".to_string()));

        let cli = Cli {
            options: CliOptions {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
                always_replace: false,
                verbose: false,
                open_browser: false,
            },
            client,
        };

        let got = cli.run().await;
        assert_eq!(Ok(()), got);
    }

    #[actix_rt::test]
    async fn test_cli_get_long_url() {
        let mut client = MockClient::new();
        client.want_get_long_url_called_with = Some("hi".to_string());

        let cli = Cli {
            options: CliOptions {
                shorturl: "hi".to_string(),
                target: None,
                always_replace: false,
                verbose: false,
                open_browser: false,
            },
            client,
        };

        let got = cli.run().await;
        assert_eq!(Ok(()), got);
    }
}

#[cfg(test)]
mod cli_errors_test {
    use super::*;

    struct MockClient {
        create_new_called_with: Option<(String, String)>,
        want_create_new_called_with: Option<(String, String)>,

        update_url_called_with: Option<(String, String)>,
        want_update_url_called_with: Option<(String, String)>,

        get_long_url_called_with: Option<String>,
        want_get_long_url_called_with: Option<String>,
    }

    impl MockClient {
        fn new() -> Self {
            MockClient {
                create_new_called_with: None,
                want_create_new_called_with: None,

                update_url_called_with: None,
                want_update_url_called_with: None,

                get_long_url_called_with: None,
                want_get_long_url_called_with: None,
            }
        }
    }

    #[async_trait]
    impl Client for MockClient {
        async fn create_new(mut self, shorturl: String, target: String) -> Result<(), GoToError> {
            self.create_new_called_with = Some((shorturl, target));
            Ok(())
        }

        async fn update_url(mut self, shorturl: String, target: String) -> Result<(), GoToError> {
            self.update_url_called_with = Some((shorturl, target));
            Ok(())
        }

        async fn get_long_url(mut self, shorturl: String) -> Result<String, GoToError> {
            self.get_long_url_called_with = Some(shorturl);
            Ok(String::new())
        }
    }

    impl Drop for MockClient {
        fn drop(&mut self) {
            let want = self.want_create_new_called_with.as_ref();
            let got = self.create_new_called_with.as_ref();
            assert_eq!(want, got);

            let want = self.want_update_url_called_with.as_ref();
            let got = self.update_url_called_with.as_ref();
            assert_eq!(want, got);

            let want = self.want_get_long_url_called_with.as_ref();
            let got = self.get_long_url_called_with.as_ref();
            assert_eq!(want, got);
        }
    }

    #[actix_rt::test]
    async fn test_cli_create_new() {
        let mut client = MockClient::new();
        client.want_create_new_called_with =
            Some(("hello".to_string(), "http://world".to_string()));

        let cli = Cli {
            options: CliOptions {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
                always_replace: false,
                verbose: false,
                open_browser: false,
            },
            client,
        };
        cli.run().await.unwrap()
    }

    #[actix_rt::test]
    async fn test_cli_update_existing() {
        let mut client = MockClient::new();
        client.want_update_url_called_with =
            Some(("hello".to_string(), "http://world".to_string()));

        let cli = Cli {
            options: CliOptions {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
                always_replace: true,
                verbose: false,
                open_browser: false,
            },
            client,
        };
        cli.run().await.unwrap()
    }

    #[actix_rt::test]
    async fn test_cli_get_long_url() {
        let mut client = MockClient::new();
        client.want_get_long_url_called_with = Some("hi".to_string());

        let cli = Cli {
            options: CliOptions {
                shorturl: "hi".to_string(),
                target: None,
                always_replace: false,
                verbose: false,
                open_browser: false,
            },
            client,
        };
        cli.run().await.unwrap()
    }
}

struct HttpClient {
    base_url: String,
}

impl HttpClient {
    fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

impl HttpClient {
    async fn create_short_url(
        self,
        shorturl: String,
        target: String,
        method: Method,
    ) -> Result<(), GoToError> {
        let client = HyperClient::new();

        let uri = format!("{}/{}", self.base_url, shorturl).parse::<Uri>()?;
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::from(target))
            .map_err(|err| GoToError::CliError(err.to_string()))?;

        let resp = client
            .request(req)
            .await
            .map_err(|err| GoToError::ApiError(err.to_string()))?;

        let is_server_error = resp.status().is_server_error();
        let is_client_error = resp.status().is_client_error();
        if is_server_error || is_client_error {
            use hyper::body::HttpBody as _;
            let body = resp.into_body().data().await.unwrap().unwrap().to_vec();
            let body = String::from_utf8(body)?;

            if is_server_error {
                return Err(GoToError::ApiError(body));
            } else {
                return Err(GoToError::CliError(body));
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Client for HttpClient {
    async fn create_new(self, shorturl: String, target: String) -> Result<(), GoToError> {
        self.create_short_url(shorturl, target, Method::POST).await
    }

    async fn update_url(self, shorturl: String, target: String) -> Result<(), GoToError> {
        self.create_short_url(shorturl, target, Method::PUT).await
    }

    async fn get_long_url(self, shorturl: String) -> Result<String, GoToError> {
        let client = HyperClient::new();
        let uri = format!("{}/{}", self.base_url, shorturl).parse::<Uri>()?;

        let resp = client
            .get(uri)
            .await
            .map_err(|err| GoToError::ApiError(err.to_string()))?;

        if !resp.status().is_redirection() {
            let is_server_error = resp.status().is_server_error();
            let is_client_error = resp.status().is_client_error();
            if is_server_error || is_client_error {
                use hyper::body::HttpBody as _;
                let body = resp.into_body().data().await.unwrap().unwrap().to_vec();
                let body = String::from_utf8(body)?;

                if is_server_error {
                    return Err(GoToError::ApiError(body));
                } else {
                    return Err(GoToError::CliError(body));
                }
            }

            return Err(GoToError::NoRedirection);
        }

        let location = resp
            .headers()
            .get("location")
            .ok_or(GoToError::NoRedirection)?;

        Ok(location.to_str()?.to_string())
    }
}

#[test]
fn test_from_tostrerror() {
    let header = hyper::header::HeaderValue::from_bytes(b"Hello \xF0\x90\x80World").unwrap();

    let res = header.to_str();
    assert!(res.is_err());

    let got = GoToError::from(res.err().unwrap());
    assert_eq!(
        GoToError::ApiError("failed to convert header to a str".to_string()),
        got
    );
}

#[cfg(test)]
mod http_client_tests {
    use super::*;

    use httpmock::{Method, MockServer};

    #[actix_rt::test]
    async fn test_create_new() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::POST).path("/sdfsdf");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url());
        client
            .create_new("sdfsdf".to_string(), "http://target.com".to_string())
            .await
            .unwrap();

        mock.assert();
    }

    #[actix_rt::test]
    async fn test_update_url() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::PUT).path("/sdfsdf");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url());
        client
            .update_url("sdfsdf".to_string(), "http://target.com".to_string())
            .await
            .unwrap();

        mock.assert();
    }

    #[actix_rt::test]
    async fn test_create_new_client_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::POST).path("/sdfsdf");

            then.status(400).body("è_é");
        });

        let client = HttpClient::new(server.base_url());
        let res = client
            .create_new("sdfsdf".to_string(), "http://target.com".to_string())
            .await;

        mock.assert();
        assert_eq!(Err(GoToError::CliError("è_é".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_create_new_api_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::POST).path("/sdfsdf");

            then.status(500).body("woops");
        });

        let client = HttpClient::new(server.base_url());
        let res = client
            .create_new("sdfsdf".to_string(), "http://target.com".to_string())
            .await;

        mock.assert();
        assert_eq!(Err(GoToError::ApiError("woops".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_create_new_not_utf8_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::POST).path("/qqqqq");

            then.status(500).body([0, 159, 146, 150]);
        });

        let client = HttpClient::new(server.base_url());
        let res = client
            .create_new("qqqqq".to_string(), "http://target.com".to_string())
            .await;

        mock.assert();
        assert_eq!(
            Err(GoToError::ApiError(
                "expected utf8: invalid utf-8 sequence of 1 bytes from index 1".to_string(),
            )),
            res
        );
    }

    #[actix_rt::test]
    async fn test_get_long_url() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl3");

            then.status(302)
                .header("location", "http://hi.there")
                .body("bla bla bla");
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl3".to_string()).await.unwrap();

        mock.assert();
        assert_eq!("http://hi.there", res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_api_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl4");

            then.status(500).body("oh no");
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::ApiError("oh no".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_client_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl4");

            then.status(400).body("oh no!!");
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::CliError("oh no!!".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_no_redirection_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl4");

            then.status(200);
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::NoRedirection), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_no_redirection_err_2() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl4");

            then.status(302);
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::NoRedirection), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_not_utf8_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::GET).path("/shorturl4");

            then.status(500).body([0, 159, 146, 150]);
        });

        let client = HttpClient::new(server.base_url());
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(
            Err(GoToError::ApiError(
                "expected utf8: invalid utf-8 sequence of 1 bytes from index 1".to_string(),
            )),
            res
        );
    }

    #[actix_rt::test]
    async fn test_get_long_url_invalid_uri() {
        let client = HttpClient::new("this is an invalid url".to_string());
        let res = client.get_long_url("shorturl4".to_string()).await;

        assert_eq!(
            Err(GoToError::CliError("invalid uri character".to_string())),
            res
        );
    }
}
