# rest2smtp

Send mails via REST API

API documentation is served by the program under the root path (`/`).
It's also available [here](https://petstore.swagger.io/?url=https://raw.githubusercontent.com/knrdl/rest2smtp/main/www/openapi.yaml#/mail/sendmail).

## Config

| Env Var         | Description                                                                                                         |
|-----------------|---------------------------------------------------------------------------------------------------------------------|
| SMTP_HOST       | Hostname (required)                                                                                                 |
| SMTP_PORT       | (default depends on encryption method)                                                                              |
| SMTP_ENCRYPTION | `TLS` (default), `STARTTLS`, `UNENCRYPTED` (insecure)                                                               |
| SMTP_USERNAME   | (optional)                                                                                                          |
| SMTP_PASSWORD   | (optional)                                                                                                          |
| API_TOKEN       | When set, HTTP request header `Authorization: Bearer <token>` must be present. (optional)                           |
| API_DOC_INFO    | Custom text (or HTML) to be displayed in API documentation header. Defaults to "Send mails via REST API" (optional) |

## Deployment

### Docker

```shell
docker run -p 8080:80 -e SMTP_HOST=smtp.example.org knrdl/rest2smtp
```

Open the API documentation: http://localhost:8080/

### Docker Compose

```yaml
version: '3.9'

services:
  rest2smtp:
    image: knrdl/rest2smtp  # or alternative: ghcr.io/knrdl/rest2smtp
    hostname: rest2smtp
    environment:
      SMTP_HOST: smtp.example.org  # replace this
      # see config table above for optional settings
    ports:
      - "80:80"
```

## Manual build

```shell
# in project root dir
docker run -it --rm -v "$PWD:$PWD" -w "$PWD" -p8080:80 --env-file env docker.io/library/rust
$ cargo run
```
