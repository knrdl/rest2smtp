use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};

/// Optional shared API token loaded from `API_TOKEN`.
/// When `None`, authentication is disabled.
#[derive(Debug, Clone)]
pub struct ApiTokenConfig {
    pub token: Option<String>,
}

impl ApiTokenConfig {
    pub fn from_env() -> Self {
        let token = std::env::var("API_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        Self { token }
    }

    pub fn enabled(&self) -> bool {
        self.token.is_some()
    }
}

/// Request guard that enforces bearer-token auth when `API_TOKEN` is set.
pub struct ApiAuth;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiAuth {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(config) = req.rocket().state::<ApiTokenConfig>() else {
            return Outcome::Success(ApiAuth);
        };

        let Some(expected) = &config.token else {
            return Outcome::Success(ApiAuth);
        };

        let provided = req
            .headers()
            .get_one("Authorization")
            .and_then(|header| header.strip_prefix("Bearer ").map(str::trim));

        match provided {
            Some(token) if tokens_equal(token, expected) => Outcome::Success(ApiAuth),
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

fn tokens_equal(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    provided
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}
