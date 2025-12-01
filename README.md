# Goto

[![codecov](https://codecov.io/gh/tsauvajon/goto/branch/master/graph/badge.svg?token=998InbDC0M)](https://codecov.io/gh/tsauvajon/goto)

Goto is a service designed to shorten URLs.  
It is an HTTP API server which stores data in-memory and can optionally persist
it to disk.

2 clients are available for it, a front-end web interface and a CLI tool.

## Installation

### Server
```sh
make install-api # -> install binary at $HOME/.local/bin/goto-api
goto-api --version
```

### Server: Launchd service (MacOS)
```sh
make install-macos
echo "api_url: http://127.0.0.1:50002" > $HOME/.config/goto/config.yml
goto-api --version
```

### Client: CLI tool
```sh
make install # -> install at $HOME/.local/bin/goto
goto --version

goto hello http://world.com
goto hello # Should open http://world.com in the browser
```

Check `Makefile` for all possibilities.

## Running without installation

If you'd rather not install the binaries, you can run them directly via Cargo:

## Server

```sh
cargo run

# or

cargo build --release
cd target/release
goto-api
```

With options:
```sh
goto-api --addr 127.0.0.1:8080 --database ./database.yml --frontdir front/dist/
```

Use `cargo run -- --help` for available options and their description.

## Clients

### CLI tool

![CLI tool demo](/demo-cli.gif)

#### Build it yourself
```sh
cargo build --bin goto
target/debug/goto --version
```

The first time you run the CLI, it will create its configuration at
`$HOME/.config/goto/config.yml`. Feel free to edit it to change the defaults!

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

# replace an existing URL
goto hello http://planet --force
```

#### Clean-up

```sh
rm $HOME/.local/bin/goto
rm $HOME/.local/bin/goto-api
rm -rf $HOME/.config/goto
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
