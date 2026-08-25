use std::{convert::identity, fmt::Debug, fs::OpenOptions, path::PathBuf};

use async_trait::async_trait;
#[cfg(all(not(coverage), not(tarpaulin_include)))]
use home::home_dir;
use hyper::{Body, Client as HyperClient, Method, Request, Uri};
use serde::{Deserialize, Serialize};
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
            api_key: None,
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
            api_key: None,
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
            api_key: None,
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
        let Some(target) = self.options.target else {
            let location = self.client.get_long_url(self.options.shorturl).await?;

            display_location(&location, self.options.verbose, &mut std::io::stdout());
            open_location(&location, self.options.open_browser);

            return Ok(());
        };

        if self.options.always_replace {
            self.client.update_url(self.options.shorturl, target).await
        } else {
            self.client.create_new(self.options.shorturl, target).await
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

#[cfg(any(coverage, tarpaulin_include))]
fn open_location(_loc: &str, _browser: bool) {}

#[cfg(all(not(coverage), not(tarpaulin_include)))]
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
    /// Optional credential for state-changing requests, sent as
    /// `Authorization: Basic base64(api_key)`.
    ///
    /// For an Authentik forward-auth edge, this is the Authentik username and
    /// an application password joined with a colon (`username:app-password`).
    /// GET requests never carry it.
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

trait ReadWrite: std::io::Read + std::io::Write {}

impl<T: std::io::Read + std::io::Write> ReadWrite for T {}

impl Default for Config {
    fn default() -> Self {
        Self {
            silent: Some(false),
            force_replace: Some(false),
            no_browser: Some(false),
            api_url: Some(DEFAULT_API_URL.to_string()),
            api_key: None,
        }
    }
}

fn open_or_create_config(filepath: &PathBuf) -> Result<Config, GoToError> {
    let _ = std::fs::create_dir_all(filepath.parent().unwrap());

    // Read-only first: works on read-only filesystems (e.g. /nix/store) where
    // an existing valid config does not need to be rewritten.
    let existing_config = if filepath.exists() {
        let buf = std::fs::read_to_string(filepath)
            .map_err(|err| GoToError::CliError(format!("read config file: {err}")))?;
        if buf.is_empty() {
            None
        } else {
            Some(
                serde_yaml::from_str(&buf)
                    .map_err(|err| GoToError::CliError(format!("parse config data: {err}"))),
            )
        }
    } else {
        None
    };

    match existing_config {
        Some(config) => config,
        None => {
            // File missing or empty: create it and populate with defaults.
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .read(true)
                .truncate(false)
                .open(filepath)
                .map_err(|err| GoToError::CliError(format!("open config file: {err}")))?;

            read_or_write_config(&mut file)
        }
    }
}

fn read_or_write_config(file: &mut dyn ReadWrite) -> Result<Config, GoToError> {
    let mut buf = String::new();
    match file.read_to_string(&mut buf) {
        Err(err) => Err(GoToError::CliError(format!("read config file: {err}"))),
        Ok(len) => {
            if len == 0 {
                let default_config = Config::default();
                file.write_all(serde_yaml::to_string(&default_config).unwrap().as_bytes())
                    .map_err(|err| GoToError::CliError(format!("write default config: {err}",)))?;

                Ok(default_config)
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
    use std::{
        env::temp_dir,
        fs::File,
        io::{Cursor, Error, Read, Result, Write},
    };

    use super::*;

    #[test]
    fn test_default_config() {
        let default = Config::default();

        assert_eq!(Some(false), default.silent);
        assert_eq!(Some(false), default.force_replace);
        assert_eq!(Some(false), default.no_browser);
        assert_eq!(
            Some(DEFAULT_API_URL.to_string()),
            default.api_url,
            "default api url should be localhost"
        );
    }

    #[test]
    fn test_create_config_when_missing() {
        let mut data: Vec<u8> = Vec::new();
        let mut mock_file = Cursor::new(&mut data);

        read_or_write_config(&mut mock_file).unwrap();

        let got = String::from_utf8(data).unwrap();
        assert!(got.contains("silent: false"), "{}", got);
        assert!(got.contains("no_browser: false"), "{}", got);
        assert!(got.contains("api_url: http://"), "{}", got);
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
    fn test_read_config_with_wrong_shape() {
        // Valid utf-8 that is not a config map must fail with a parse error.
        let mut data: Vec<u8> = Vec::from("[1, 2, 3]");
        let mut mock_file = Cursor::new(&mut data);

        let err = read_or_write_config(&mut mock_file).unwrap_err();

        assert_eq!(
            GoToError::CliError(
                "parse config data: invalid type: sequence, expected struct Config".to_string()
            ),
            err
        );
    }

    #[test]
    fn test_read_legacy_config_without_api_key() {
        let mut data: Vec<u8> = Vec::from("api_url: \"https://go.example\"\nsilent: false");
        let mut mock_file = Cursor::new(&mut data);

        let got = read_or_write_config(&mut mock_file).unwrap();

        assert_eq!(Some("https://go.example".to_string()), got.api_url);
        assert_eq!(Some(false), got.silent);
        assert_eq!(None, got.api_key, "legacy configs must keep deserializing");
    }

    #[test]
    fn test_read_config_with_api_key() {
        let mut data: Vec<u8> =
            Vec::from("api_url: \"https://go.example\"\napi_key: \"thomas:hunter2\"");
        let mut mock_file = Cursor::new(&mut data);

        let got = read_or_write_config(&mut mock_file).unwrap();

        assert_eq!(Some("thomas:hunter2".to_string()), got.api_key);
    }

    #[test]
    fn test_default_config_omits_empty_api_key() {
        let serialized = serde_yaml::to_string(&Config::default()).unwrap();

        assert!(!serialized.contains("api_key"), "{}", serialized);
    }

    #[test]
    fn test_create_config() {
        let mut filepath = temp_dir();
        filepath.push("test_create_config.yml");
        // Start clean: a file left by a previous run would take the
        // read-existing path instead of the creation path under test.
        let _ = std::fs::remove_file(&filepath);

        let got = open_or_create_config(&filepath);
        assert!(got.is_ok());

        let mut file = File::open(&filepath).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();

        assert!(content.contains("api_url"));
    }

    #[test]
    fn test_existing_empty_config_is_recreated_with_defaults() {
        let mut filepath = temp_dir();
        filepath.push("test_existing_empty_config.yml");
        let _ = std::fs::remove_file(&filepath);

        File::create(&filepath).unwrap();
        assert!(filepath.exists());

        let got = open_or_create_config(&filepath).unwrap();
        assert_eq!(Config::default(), got);
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
    fn test_open_readonly_config() {
        // Regression: reading a valid config from a read-only file (e.g. when the
        // file is symlinked into /nix/store) must not fail.
        let mut filepath = temp_dir();
        filepath.push("test_open_readonly_config.yml");

        // Make sure we start clean even if a previous run left the file behind.
        let _ = std::fs::remove_file(&filepath);

        let mut file = File::create(&filepath).unwrap();
        file.write_all(b"api_url: \"http://readonly.example\"\n")
            .unwrap();
        drop(file);

        let mut perms = std::fs::metadata(&filepath).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&filepath, perms).unwrap();

        let got = open_or_create_config(&filepath).unwrap();
        assert_eq!(Some("http://readonly.example".to_string()), got.api_url);

        // Restore writable perms so the temp file can be cleaned up by the OS.
        let mut perms = std::fs::metadata(&filepath).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(&filepath, perms);
        let _ = std::fs::remove_file(&filepath);
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

    #[test]
    fn test_open_unreadable_existing_config() {
        // An existing entry that cannot be read back (here: a directory) must
        // surface a read-config-file error instead of being recreated.
        let mut filepath = temp_dir();
        filepath.push("test_open_unreadable_existing_config.d");
        std::fs::create_dir_all(&filepath).unwrap();

        let got = open_or_create_config(&filepath);
        assert!(got.is_err());

        let err = got.err().unwrap();
        assert!(
            format!("{:?}", err).contains("read config file:"),
            "{:?}",
            err
        );

        let _ = std::fs::remove_dir(&filepath);
    }

    struct RWMock {
        read_err: Option<Error>,
        write_err: Option<Error>,
        data: Vec<u8>,
    }

    impl std::io::Read for RWMock {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
            match self.read_err.take() {
                Some(err) => Err(err),
                None => Ok(0),
            }
        }
    }

    impl std::io::Write for RWMock {
        fn write(&mut self, buf: &[u8]) -> Result<usize> {
            if let Some(err) = self.write_err.take() {
                return Err(err);
            }

            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_cannot_read_config() {
        let mut mock_file = RWMock {
            read_err: Some(Error::other("oh no!")),
            write_err: None,
            data: Vec::new(),
        };

        let got = read_or_write_config(&mut mock_file);
        let want = Err(GoToError::CliError("read config file: oh no!".to_string()));
        assert_eq!(want, got);
    }

    #[test]
    fn test_cannot_write_config() {
        let mut mock_file = RWMock {
            read_err: None,
            write_err: Some(Error::other("that went terribly wrong!")),
            data: Vec::new(),
        };

        let got = read_or_write_config(&mut mock_file);
        let want = Err(GoToError::CliError(
            "write default config: that went terribly wrong!".to_string(),
        ));
        assert_eq!(want, got);
    }

    #[test]
    fn test_rwmock_success_path() {
        let mut mock_file = RWMock {
            read_err: None,
            write_err: None,
            data: Vec::new(),
        };

        let got = read_or_write_config(&mut mock_file).unwrap();
        assert_eq!(Config::default(), got);
        assert!(!mock_file.data.is_empty());
        mock_file.flush().unwrap();
    }
}

#[cfg(test)]
mod cant_read_config_tests {}

#[tokio::main]
#[cfg(all(not(coverage), not(tarpaulin_include)))]
async fn main() -> Result<(), GoToError> {
    let args = Args::from_args();

    let mut filepath = home_dir().unwrap();
    filepath.push(".config");
    filepath.push("goto");
    filepath.push("config.yml");

    let config = open_or_create_config(&filepath).unwrap();

    create_cli(&args, &config).run().await
}

fn create_cli(args: &Args, config: &Config) -> Cli<HttpClient> {
    let options = CliOptions::new(args, config);
    let api_url = get_api_url(args, config);

    Cli {
        options,
        client: HttpClient::new(api_url, config.api_key.clone()),
    }
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
        api_key: None,
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

#[test]
fn test_create_cli_propagates_api_key_to_client() {
    let args = Args {
        shorturl: String::new(),
        target: None,
        api_url: None,
        force_replace: false,
        silent: false,
        no_browser: false,
    };

    // configured key reaches the HTTP client for mutating requests
    let config = Config {
        api_url: None,
        force_replace: None,
        silent: None,
        no_browser: None,
        api_key: Some("thomas:hunter2".to_string()),
    };
    let cli = create_cli(&args, &config);
    assert_eq!(Some("thomas:hunter2".to_string()), cli.client.api_key);

    // absent key stays absent
    let config = Config {
        api_key: None,
        ..config
    };
    let cli = create_cli(&args, &config);
    assert_eq!(None, cli.client.api_key);
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
        get_long_url_result: Option<Result<String, GoToError>>,
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
                get_long_url_result: Some(Ok(String::new())),
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
            self.get_long_url_result.take().unwrap()
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

        let got = cli.run().await;
        assert_eq!(Ok(()), got);
    }

    #[actix_rt::test]
    async fn test_cli_get_long_url_error() {
        let mut client = MockClient::new();
        client.want_get_long_url_called_with = Some("hi".to_string());
        client.get_long_url_result = Some(Err(GoToError::ApiError("boom".to_string())));

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
        assert_eq!(Err(GoToError::ApiError("boom".to_string())), got);
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
        get_long_url_result: Option<Result<String, GoToError>>,
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
                get_long_url_result: Some(Ok(String::new())),
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
            self.get_long_url_result.take().unwrap()
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

    #[actix_rt::test]
    async fn test_cli_get_long_url_error() {
        let mut client = MockClient::new();
        client.want_get_long_url_called_with = Some("hi".to_string());
        client.get_long_url_result = Some(Err(GoToError::ApiError("boom".to_string())));

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
        assert_eq!(Err(GoToError::ApiError("boom".to_string())), got);
    }
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Build a hyper client with TLS support so `https://` API URLs work.
fn https_client() -> HyperClient<hyper_tls::HttpsConnector<hyper::client::HttpConnector>, Body> {
    HyperClient::builder().build(hyper_tls::HttpsConnector::new())
}

/// Encode `data` as standard base64 with padding (RFC 4648).
fn base64_encode(data: &[u8]) -> String {
    let mut encoded = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;

        encoded.push(BASE64_ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        encoded.push(BASE64_ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            BASE64_ALPHABET[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

#[test]
fn test_base64_encode_rfc4648_vectors() {
    assert_eq!("", base64_encode(b""));
    assert_eq!("Zg==", base64_encode(b"f"));
    assert_eq!("Zm8=", base64_encode(b"fo"));
    assert_eq!("Zm9v", base64_encode(b"foo"));
    assert_eq!("Zm9vYg==", base64_encode(b"foob"));
    assert_eq!("Zm9vYmE=", base64_encode(b"fooba"));
    assert_eq!("Zm9vYmFy", base64_encode(b"foobar"));
}

struct HttpClient {
    base_url: String,
    api_key: Option<String>,
}

impl HttpClient {
    fn new(base_url: String, api_key: Option<String>) -> Self {
        Self { base_url, api_key }
    }

    async fn create_short_url(
        self,
        shorturl: String,
        target: String,
        method: Method,
    ) -> Result<(), GoToError> {
        let client = https_client();

        let req = build_mutation_request(
            &self.base_url,
            &shorturl,
            target,
            method,
            self.api_key.as_deref(),
        )?;

        let resp = client
            .request(req)
            .await
            .map_err(|err| GoToError::ApiError(err.to_string()))?;

        // A mutation answered by a redirect means the edge sent us to an
        // authentication flow (missing or rejected credentials): never
        // treat that as success.
        if resp.status().is_redirection() {
            return Err(GoToError::CliError(
                "authentication required: the server redirected the request; set api_key in the goto config"
                    .to_string(),
            ));
        }

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

/// Build the request for a state-changing call (`POST`/`PUT`).
///
/// When `api_key` is set, exactly one `Authorization: Basic <base64>` header
/// is attached. Resolution requests never go through here.
fn build_mutation_request(
    base_url: &str,
    shorturl: &str,
    target: String,
    method: Method,
    api_key: Option<&str>,
) -> Result<Request<Body>, GoToError> {
    let uri = format!("{}/{}", base_url, shorturl).parse::<Uri>()?;
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(api_key) = api_key {
        builder = builder.header(
            hyper::header::AUTHORIZATION,
            format!("Basic {}", base64_encode(api_key.as_bytes())),
        );
    }
    Ok(builder
        .body(Body::from(target))
        .expect("request builder should not fail for valid method, uri, and body"))
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
        let client = https_client();
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
    use httpmock::{Method, MockServer};

    use super::*;

    #[test]
    fn test_build_mutation_request_attaches_basic_auth_exactly_once() {
        let req = build_mutation_request(
            "http://localhost",
            "hello",
            "http://target".to_string(),
            hyper::Method::POST,
            Some("thomas:secret"),
        )
        .unwrap();

        assert_eq!(&hyper::Method::POST, req.method());
        assert_eq!(
            &hyper::Uri::from_static("http://localhost/hello"),
            req.uri()
        );
        let headers = req.headers().get_all(hyper::header::AUTHORIZATION);
        assert_eq!(1, headers.iter().count());
        assert_eq!(
            hyper::header::HeaderValue::from_static("Basic dGhvbWFzOnNlY3JldA=="),
            headers.iter().next().unwrap()
        );
    }

    #[test]
    fn test_build_mutation_request_without_api_key_has_no_authorization() {
        let req = build_mutation_request(
            "http://localhost",
            "hello",
            "http://target".to_string(),
            hyper::Method::PUT,
            None,
        )
        .unwrap();

        assert_eq!(&hyper::Method::PUT, req.method());
        assert!(req.headers().get(hyper::header::AUTHORIZATION).is_none());
    }

    #[test]
    fn test_build_mutation_request_invalid_uri() {
        let res = build_mutation_request(
            "this is an invalid url",
            "hello",
            "http://target".to_string(),
            hyper::Method::POST,
            Some("thomas:secret"),
        );

        let err = res.unwrap_err();
        assert_eq!(
            GoToError::CliError("invalid uri character".to_string()),
            err
        );
    }

    #[actix_rt::test]
    async fn test_create_new_sends_basic_auth_when_configured() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/sdfsdf")
                .header("Authorization", "Basic dGhvbWFzOnNlY3JldA==");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url(), Some("thomas:secret".to_string()));
        client
            .create_new("sdfsdf".to_string(), "http://target.com".to_string())
            .await
            .unwrap();

        mock.assert();
    }

    #[actix_rt::test]
    async fn test_update_url_sends_basic_auth_when_configured() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::PUT)
                .path("/sdfsdf")
                .header("Authorization", "Basic dGhvbWFzOnNlY3JldA==");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url(), Some("thomas:secret".to_string()));
        client
            .update_url("sdfsdf".to_string(), "http://target.com".to_string())
            .await
            .unwrap();

        mock.assert();
    }

    #[actix_rt::test]
    async fn test_get_long_url_never_sends_authorization_even_when_configured() {
        let server = MockServer::start();

        // Matches any GET carrying an Authorization header; if the client ever
        // leaks credentials on resolution this mock answers 500 and fails the test.
        let leak = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .header_exists("Authorization");

            then.status(500).body("credential leaked");
        });
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl5");

            then.status(302)
                .header("location", "http://hi.there")
                .body("bla bla bla");
        });

        let client = HttpClient::new(server.base_url(), Some("thomas:secret".to_string()));
        let res = client.get_long_url("shorturl5".to_string()).await.unwrap();

        assert_eq!("http://hi.there", res);
        assert_eq!(0, leak.calls());
        mock.assert();
    }

    #[actix_rt::test]
    async fn test_create_new_redirect_means_authentication_required() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST).path("/sdfsdf");

            then.status(302)
                .header("location", "https://auth.example/start");
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client
            .create_new("sdfsdf".to_string(), "http://target.com".to_string())
            .await;

        mock.assert();
        match res {
            Ok(_) => panic!("a redirected mutation must not be reported as success"),
            Err(err) => assert!(
                matches!(err, GoToError::CliError(ref m) if m.contains("authentication required"))
            ),
        }
    }

    #[actix_rt::test]
    async fn test_create_new() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(Method::POST).path("/sdfsdf");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url(), None);
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
            when.method(httpmock::Method::PUT).path("/sdfsdf");

            then.status(200).body("ok!!");
        });

        let client = HttpClient::new(server.base_url(), None);
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

        let client = HttpClient::new(server.base_url(), None);
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

        let client = HttpClient::new(server.base_url(), None);
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

        let client = HttpClient::new(server.base_url(), None);
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
            when.method(httpmock::Method::GET).path("/shorturl3");

            then.status(302)
                .header("location", "http://hi.there")
                .body("bla bla bla");
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client.get_long_url("shorturl3".to_string()).await.unwrap();

        mock.assert();
        assert_eq!("http://hi.there", res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_api_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl4");

            then.status(500).body("oh no");
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::ApiError("oh no".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_client_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl4");

            then.status(400).body("oh no!!");
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::CliError("oh no!!".to_string())), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_no_redirection_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl4");

            then.status(200);
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::NoRedirection), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_no_redirection_err_2() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl4");

            then.status(302);
        });

        let client = HttpClient::new(server.base_url(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        mock.assert();
        assert_eq!(Err(GoToError::NoRedirection), res);
    }

    #[actix_rt::test]
    async fn test_get_long_url_not_utf8_err() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/shorturl4");

            then.status(500).body([0, 159, 146, 150]);
        });

        let client = HttpClient::new(server.base_url(), None);
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
        let client = HttpClient::new("this is an invalid url".to_string(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        assert_eq!(
            Err(GoToError::CliError("invalid uri character".to_string())),
            res
        );
    }

    #[actix_rt::test]
    async fn test_get_long_url_transport_err() {
        let client = HttpClient::new("http://127.0.0.1:1".to_string(), None);
        let res = client.get_long_url("shorturl4".to_string()).await;

        assert!(matches!(res, Err(GoToError::ApiError(_))));
    }

    #[actix_rt::test]
    async fn test_create_new_invalid_uri() {
        let client = HttpClient::new("this is an invalid url".to_string(), None);
        let res = client
            .create_new("shorturl4".to_string(), "http://target.com".to_string())
            .await;

        assert_eq!(
            Err(GoToError::CliError("invalid uri character".to_string())),
            res
        );
    }

    #[actix_rt::test]
    async fn test_create_new_transport_err() {
        let client = HttpClient::new("http://127.0.0.1:1".to_string(), None);
        let res = client
            .create_new("shorturl4".to_string(), "http://target.com".to_string())
            .await;

        assert!(matches!(res, Err(GoToError::ApiError(_))));
    }
}
