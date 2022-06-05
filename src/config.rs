use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum SmtpEncryption {
    Tls,
    StartTls,
    Unencrypted,
}

impl fmt::Display for SmtpEncryption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SmtpEncryption::Tls => write!(f, "TLS"),
            SmtpEncryption::StartTls => write!(f, "STARTTLS"),
            SmtpEncryption::Unencrypted => write!(f, "UNENCRYPTED"),
        }
    }
}

#[derive(Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: Option<u16>,
    pub encryption: SmtpEncryption,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl SmtpConfig {
    pub fn new() -> SmtpConfig {
        let host = env::var("SMTP_HOST").expect("SMTP_HOST is not set");
        assert_eq!(host.trim().is_empty(), false, "SMTP_HOST is empty");

        SmtpConfig {
            host,
            port: if let Some(p) = env::var("SMTP_PORT").ok() { Some(p.parse::<u16>().unwrap()) } else { None },
            username: env::var("SMTP_USERNAME").ok(),
            password: env::var("SMTP_PASSWORD").ok(),
            encryption: match env::var("SMTP_ENCRYPTION").ok() {
                Some(enc) => match enc.trim().to_lowercase().as_str() {
                    "tls" => SmtpEncryption::Tls,
                    "starttls" => SmtpEncryption::StartTls,
                    "unencrypted" => SmtpEncryption::Unencrypted,
                    _ => SmtpEncryption::Tls
                },
                None => SmtpEncryption::Tls
            },
        }
    }
}

pub fn generate_api_doc() -> Result<(), io::Error> {
    let file_path = Path::new("./www/openapi.yaml");
    let contents = fs::read_to_string(&file_path)?;
    let new_content = contents.replace(
        "%%%API_DOC_INFO%%%",
        &env::var("API_DOC_INFO").unwrap_or("Send mails via REST API".to_string()).replace("'", "\""),
    );
    fs::write(&file_path, new_content)?;
    Ok(())
}
