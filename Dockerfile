FROM alpine as swagger_builder

WORKDIR /swagger
RUN apk add --no-cache git && \
    git clone --depth=1 https://github.com/swagger-api/swagger-ui && \
    cd swagger-ui/dist && \
    rm *.map
COPY www/* /swagger/swagger-ui/dist/


# platform parameter fixes https://github.com/docker/buildx/issues/395
FROM --platform=${BUILDPLATFORM:-linux/amd64} rust:1-bullseye as executable_builder

WORKDIR /usr/src/app
COPY src ./src
COPY Cargo.lock Cargo.toml ./
RUN cargo build --release && \
    strip target/release/rest2smtp


FROM debian:bullseye-slim

RUN apt-get update && \
    apt-get install -y openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

EXPOSE 80/tcp
WORKDIR /app

COPY --from=executable_builder /usr/src/app/target/release/rest2smtp /app/rest2smtp
COPY --from=swagger_builder /swagger/swagger-ui/dist /app/www
COPY Rocket.toml /app/

CMD ["/app/rest2smtp"]
