# Goto

[![codecov](https://codecov.io/gh/tsauvajon/shorturl/branch/master/graph/badge.svg?token=EbP2Znh1m3)](https://codecov.io/gh/tsauvajon/shorturl)

Goto is a service designed to shorten URLs.  
It is an HTTP API server which stores data in-memory and can optionally persist
it to disk.

2 clients are available for it, a front-end web interface and a CLI tool.

![Demo](/demo.gif)

## Example usage

## API

```sh
cargo run
```

With some options:
```sh
cargo run -- --addr 127.0.0.1:8080 --database ./database.yml --frontdir front/dist/
```

Use `cargo run -- --help` for available options and their description.

## Clients

### HTTP Client

You can query the API with any HTTP client.

```sh
$ curl -X POST 127.0.0.1:8080/tsauvajon -d "https://linkedin.com/in/tsauvajon"
/tsauvajon now redirects to https://linkedin.com/in/tsauvajon

$ curl 127.0.0.1:8080/tsauvajon
redirecting to https://linkedin.com/in/tsauvajon...
```

### Web Front-End

The front-end is designed to be served by the API, so make sure to have the API
started and running.

```sh
$ cd front/
$ make build

# it is now ready to be served by the API
$ echo http://127.0.0.1:8080/
```

You can optionally serve the front-end with your own web server and routing
strategy.

### CLI tool

```sh
goto 
```
