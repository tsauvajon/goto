use async_trait::async_trait;
use hyper::{Client as HyperClient, Uri};
use std::fmt::Debug;
use structopt::StructOpt;
use webbrowser;

#[derive(StructOpt)]
#[structopt(about = "Create shortened URLs")]
struct Args {
    #[structopt(help = "Shortened URL")]
    shorturl: String,
    #[structopt(help = "URL to shorten")]
    target: Option<String>,

    #[structopt(long = "api", help = "Base URL of the GoTo API")]
    api_url: Option<String>,

    #[structopt(short = "s", long = "silent", help = "Don't print redirections")]
    silent: bool,

    #[structopt(short = "n", long = "no-open-browser", help = "Don't open the browser")]
    no_browser: bool,
}

struct Cli<C: Client> {
    args: Args,
    client: C,
}

impl<C: Client> Cli<C> {
    async fn run(self) -> Result<(), GoToError> {
        match self.args.target {
            Some(target) => self.client.create_new(self.args.shorturl, target).await,
            None => {
                let location = self.client.get_long_url(self.args.shorturl).await?;

                display_location(&location, self.args.silent, &mut std::io::stdout());
                open_location(&location, !self.args.no_browser);

                Ok(())
            }
        }
    }
}

fn display_location(loc: &str, silent: bool, mut writer: impl std::io::Write) {
    if !silent {
        writeln!(writer, "redirecting to {}", loc).unwrap();
    }
}

#[test]
fn test_display_location_silent() {
    let mut result = Vec::new();
    display_location("hi there", true, &mut result);

    assert_eq!(b"".to_vec(), result);
}

#[test]
fn test_display_location_verbose() {
    let mut result = Vec::new();
    display_location("http://hi.there", false, &mut result);

    assert_eq!(b"redirecting to http://hi.there\n".to_vec(), result,);
}

#[cfg(not(tarpaulin_include))]
fn open_location(loc: &str, browser: bool) {
    if browser {
        webbrowser::open(loc).unwrap();
    }
}

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<(), GoToError> {
    let args = Args::from_args();
    let url = match &args.api_url {
        Some(url) => url.to_owned(),
        None => "http://127.0.0.1:8080".to_string(),
    };

    let cli = Cli {
        args,
        client: HttpClient::new(url),
    };

    cli.run().await
}

#[async_trait]
trait Client {
    async fn create_new(self, shorturl: String, target: String) -> Result<(), GoToError>;

    async fn get_long_url(self, shorturl: String) -> Result<String, GoToError>;
}

#[cfg(test)]
mod cli_test {
    use super::*;

    struct MockClient {
        create_new_called_with: Option<(String, String)>,
        want_create_new_called_with: Option<(String, String)>,

        get_long_url_called_with: Option<String>,
        want_get_long_url_called_with: Option<String>,
    }

    impl MockClient {
        fn new() -> Self {
            MockClient {
                create_new_called_with: None,
                want_create_new_called_with: None,

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
            args: Args {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
                api_url: None,
                silent: true,
                no_browser: true,
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
            args: Args {
                shorturl: "hi".to_string(),
                target: None,
                api_url: None,
                silent: true,
                no_browser: true,
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

        get_long_url_called_with: Option<String>,
        want_get_long_url_called_with: Option<String>,
    }

    impl MockClient {
        fn new() -> Self {
            MockClient {
                create_new_called_with: None,
                want_create_new_called_with: None,

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
            args: Args {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
                api_url: Some("http://12.34.56.78".to_string()),
                silent: true,
                no_browser: true,
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
            args: Args {
                shorturl: "hi".to_string(),
                target: None,
                api_url: None,
                silent: true,
                no_browser: true,
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

#[async_trait]
impl Client for HttpClient {
    async fn create_new(self, shorturl: String, target: String) -> Result<(), GoToError> {
        let client = HyperClient::new();

        let uri = format!("{}/{}", self.base_url, shorturl).parse::<Uri>()?;

        use hyper::{Body, Method, Request};
        let req = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Body::from(target))
            .or_else(|err| Err(GoToError::CliError(err.to_string())))?;

        let resp = client
            .request(req)
            .await
            .or_else(|err| Err(GoToError::ApiError(err.to_string())))?;

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

    async fn get_long_url(self, shorturl: String) -> Result<String, GoToError> {
        let client = HyperClient::new();
        let uri = format!("{}/{}", self.base_url, shorturl).parse::<Uri>()?;

        let resp = client
            .get(uri)
            .await
            .or_else(|err| Err(GoToError::ApiError(err.to_string())))?;

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
        GoToError::ApiError(format!("expected utf8: {}", error.to_string()))
    }
}

impl From<hyper::header::ToStrError> for GoToError {
    fn from(error: hyper::header::ToStrError) -> Self {
        GoToError::ApiError(error.to_string())
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

            then.status(500).body(&[0, 159, 146, 150]);
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

            then.status(500).body(&[0, 159, 146, 150]);
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
