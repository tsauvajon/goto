use async_trait::async_trait;
use hyper::Client as HyperClient;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
    shorturl: String,
    target: Option<String>,
}

struct Cli<C: Client> {
    args: Args,
    client: C,
}

impl<C: Client> Cli<C> {
    async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.args.target {
            Some(target) => self.client.create_new(self.args.shorturl, target).await,
            None => self.client.get_long_url(self.args.shorturl).await,
        }
    }
}

#[cfg(not(tarpaulin_include))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::from_args();
    let cli = Cli {
        args,
        client: HttpClient::new(),
    };
    cli.run().await
}

#[async_trait]
trait Client {
    async fn create_new(
        self,
        shorturl: String,
        target: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn get_long_url(
        self,
        shorturl: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[cfg(test)]
mod client_test {
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
        async fn create_new(
            mut self,
            shorturl: String,
            target: String,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.create_new_called_with = Some((shorturl, target));
            Ok(())
        }

        async fn get_long_url(
            mut self,
            shorturl: String,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.get_long_url_called_with = Some(shorturl);
            Ok(())
        }
    }

    impl Drop for MockClient {
        fn drop(&mut self) {
            assert_eq!(
                self.want_create_new_called_with,
                self.create_new_called_with
            );
            assert_eq!(
                self.want_get_long_url_called_with,
                self.get_long_url_called_with
            );
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
            },
            client,
        };
        cli.run().await.unwrap()
    }
}

struct HttpClient {}

impl HttpClient {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Client for HttpClient {
    async fn create_new(
        self,
        shorturl: String,
        target: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = HyperClient::new();
        let uri = "http://127.0.0.1:8080/hi".parse()?;
        let resp = client.get(uri).await?;

        println!("Response: {}", resp.status());

        println!("creating new short url {} for {}", shorturl, target);

        Ok(())
    }

    async fn get_long_url(
        self,
        shorturl: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("getting long url for {}", &shorturl);

        let client = HyperClient::new();
        let uri = format!("http://127.0.0.1:8080/{}", shorturl).parse()?;
        let resp = client.get(uri).await?;

        if !resp.status().is_redirection() {
            Err(NoRedirectionError::new(&shorturl))?;
        }

        let location = resp.headers().get("location");
        let location = location.ok_or(NoRedirectionError::new(&shorturl))?;

        println!("{}", location.to_str()?);

        Ok(())
    }
}

#[derive(Debug)]
struct NoRedirectionError {
    shorturl: String,
}

impl std::fmt::Display for NoRedirectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "no redirection registered for {}", self.shorturl)
    }
}

impl std::error::Error for NoRedirectionError {
    fn description(&self) -> &str {
        "no redirection registered"
    }
}

impl NoRedirectionError {
    fn new(shorturl: &str) -> Self {
        NoRedirectionError {
            shorturl: shorturl.to_string(),
        }
    }
}

#[cfg(test)]
mod http_client_tests {
    use super::*;

    #[actix_rt::test]
    async fn test_create_new() {
        let client = HttpClient::new();
        client
            .create_new("shorturl".to_string(), "http://target.com".to_string())
            .await
            .unwrap()
    }
}
