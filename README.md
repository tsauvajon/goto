# ShortURL

I can shorten URLs.

```sh
$ curl -X POST 127.0.0.1:8080/tsauvajon -d "https://linkedin.com/in/tsauvajon"
/tsauvajon now redirects to https://linkedin.com/in/tsauvajon

$ curl 127.0.0.1:8080/tsauvajon                                               
redirecting to https://linkedin.com/in/tsauvajon...
```