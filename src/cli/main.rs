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
    fn run(self) {
        match self.args.target {
            Some(target) => self.client.create_new(self.args.shorturl, target),
            None => self.client.get_long_url(self.args.shorturl),
        }
    }
}

#[cfg(not(tarpaulin_include))]
fn main() {
    let args = Args::from_args();
    let cli = Cli {
        args,
        client: HttpClient::new(),
    };
    cli.run()
}

trait Client {
    fn create_new(self, shorturl: String, target: String);
    fn get_long_url(self, shorturl: String);
}

#[cfg(test)]
mod test {
    use super::*;

    struct MockClient {
        create_new_called_with: Option<(String, String)>,
        get_long_url_called_with: Option<String>,
    }

    impl MockClient {
        fn new() -> Self {
            MockClient {
                create_new_called_with: None,
                get_long_url_called_with: None,
            }
        }
    }

    impl Client for MockClient {
        fn create_new(mut self, shorturl: String, target: String) {
            self.create_new_called_with = Some((shorturl, target))
        }

        fn get_long_url(mut self, shorturl: String) {
            self.get_long_url_called_with = Some(shorturl)
        }
    }

    #[test]
    fn test_cli_create_new() {
        impl Drop for MockClient {
            fn drop(&mut self) {
                assert_eq!(
                    Some(("hello".to_string(), "http://world".to_string())),
                    self.create_new_called_with,
                );

                assert_eq!(None, self.get_long_url_called_with);
            }
        }

        let client = MockClient::new();
        let cli = Cli {
            args: Args {
                shorturl: "hello".to_string(),
                target: Some("http://world".to_string()),
            },
            client,
        };
        cli.run();
    }

    #[test]
    fn test_cli_get_long_url() {
        impl Drop for MockClient {
            fn drop(&mut self) {
                assert_eq!(Some("hi".to_string()), self.get_long_url_called_with,);

                assert_eq!(None, self.create_new_called_with);
            }
        }

        let client = MockClient::new();
        let cli = Cli {
            args: Args {
                shorturl: "hi".to_string(),
                target: None,
            },
            client,
        };
        cli.run();
    }
}

struct HttpClient {}

impl HttpClient {
    fn new() -> Self {
        Self {}
    }
}

impl Client for HttpClient {
    fn create_new(self, shorturl: String, target: String) {
        println!("creating new short url {} for {}", shorturl, target)
    }

    fn get_long_url(self, shorturl: String) {
        println!("getting long url for {}", shorturl)
    }
}
