use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};

/// Optional shared API token loaded from `API_TOKEN` env var.
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
            .and_then(|header| extract_bearer_token(header));

        match provided {
            Some(token) if tokens_equal(token, expected) => Outcome::Success(ApiAuth),
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

fn extract_bearer_token(header_value: &str) -> Option<&str> {
    let mut parts = header_value.split_whitespace();
    let scheme = parts.next()?;

    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let token = parts.next()?;
    if parts.next().is_some() /* no whitespace allowed in credentials as of RFC 6750 */ || token.is_empty()
    {
        return None;
    }

    Some(token)
}

fn tokens_equal(provided: &str, expected: &str) -> bool {
    // time-safe comparison
    if provided.len() != expected.len() {
        return false;
    }
    provided
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn with_api_token(value: Option<&str>) -> Option<String> {
        match value {
            Some(token) => {
                env::set_var("API_TOKEN", token);
                Some(token.to_string())
            }
            None => {
                env::remove_var("API_TOKEN");
                None
            }
        }
    }

    #[test]
    fn parses_valid_bearer_tokens() {
        assert_eq!(extract_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer_token("bearer   abc123"), Some("abc123"));
    }

    #[test]
    fn rejects_malformed_authorization_headers() {
        assert_eq!(extract_bearer_token("Token abc123"), None);
        assert_eq!(extract_bearer_token("Bearer "), None);
        assert_eq!(extract_bearer_token("Bearer abc123 extra"), None);
    }

    #[test]
    fn config_from_env_handles_present_and_missing_tokens() {
        let configured = with_api_token(Some("secret-token"));
        let parsed = ApiTokenConfig::from_env();
        assert_eq!(parsed.token, configured);
        assert!(parsed.enabled());

        let removed = with_api_token(None);
        let parsed = ApiTokenConfig::from_env();
        assert_eq!(parsed.token, removed);
        assert!(!parsed.enabled());
    }
}
