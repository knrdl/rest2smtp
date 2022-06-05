# rest2smtp

Send mails via REST API

API Documentation is served by the program under the root path (`/`).

## Config

| Env Var         | Description                                                                                              |
|-----------------|----------------------------------------------------------------------------------------------------------|
| SMTP_HOST       | (required)                                                                                               |
| SMTP_PORT       | (default depends on encryption method)                                                                   |
| SMTP_ENCRYPTION | `TLS` (default), `STARTTLS`, `UNENCRYPTED` (insecure)                                                    |
| SMTP_USERNAME   | (optional)                                                                                               |
| SMTP_PASSWORD   | (optional)                                                                                               |
| API_DOC_INFO    | Custom text (or html) to be displayed in API documentation header. Defaults to "Send mails via REST API" |

## Deployment

Docker Compose / Swarm

```yaml
version: '3.9'

services:
  rest2smtp:
    image: knrdl/rest2smtp  # or alternative: ghcr.io/knrdl/rest2smtp
    hostname: rest2smtp
    environment:
      SMTP_HOST: smtp.example.org
    ports:
      - "80:80"
```
