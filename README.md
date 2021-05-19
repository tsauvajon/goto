# ShortURL

[![codecov](https://codecov.io/gh/tsauvajon/shorturl/branch/master/graph/badge.svg?token=EbP2Znh1m3)](https://codecov.io/gh/tsauvajon/shorturl)

I can shorten URLs.

```sh
$ curl -X POST 127.0.0.1:8080/tsauvajon -d "https://linkedin.com/in/tsauvajon"
/tsauvajon now redirects to https://linkedin.com/in/tsauvajon

$ curl 127.0.0.1:8080/tsauvajon                                               
redirecting to https://linkedin.com/in/tsauvajon...
```