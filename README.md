# Goto

[![codecov](https://codecov.io/gh/tsauvajon/shorturl/branch/master/graph/badge.svg?token=EbP2Znh1m3)](https://codecov.io/gh/tsauvajon/shorturl)

Goto is a service designed to shorten URLs.  
It is an HTTP API server which stores data in-memory and can optionally persist
it to disk.

2 clients are available for it, a front-end web interface and a CLI tool.

## Example usage

## Server

```sh
cargo run

# or

cargo build --release
target/release/goto-api
```

Same thing, but with some options:
```sh
cargo run -- --addr 127.0.0.1:8080 --database ./database.yml --frontdir front/dist/
```

Use `cargo run -- --help` for available options and their description.

## Clients

### CLI tool

![CLI tool demo](/demo-cli.gif)

#### Build it yourself
```sh
make build-cli
goto --version

# OR

cargo build --bin goto
target/debug/goto --version
```

The first time you run the CLI, it will create its configuration at
`$HOME/.goto/config.yml`. Feel free to edit it to change the defaults!

#### Use it

```sh
# show available options
goto --help

# create a new short URL
goto hello http://world

# browse this url, it will automatically open your web browser
goto hello

# display the URL but don't browse it
goto hello --no-open-browser
```

#### Clean-up

```sh
rm /usr/local/bin/goto
rm -rf $HOME/.goto
```

### Web Front-End

![Front-end Demo](/demo-front.gif)

The front-end is designed to be served by the API, so make sure to have the API
started and running.

```sh
$ cd front/
$ make build

# it is now ready to be served by the API
$ echo http://127.0.0.1:8080/
```

You can of course host the front-end somewhere else if you want.

### HTTP Client

You can also directly query the API with any HTTP client.

```sh
# create a new shortened URL
$ curl -X POST 127.0.0.1:8080/tsauvajon -d "https://linkedin.com/in/tsauvajon"
/tsauvajon now redirects to https://linkedin.com/in/tsauvajon

# browse it
$ curl 127.0.0.1:8080/tsauvajon
redirecting to https://linkedin.com/in/tsauvajon...
```
