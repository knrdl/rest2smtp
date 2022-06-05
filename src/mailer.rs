use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, Tokio1Executor};

use super::config::{SmtpEncryption, SmtpConfig};

pub struct Mailer {
    pub transport: AsyncSmtpTransport::<Tokio1Executor>,
    pub config: SmtpConfig,
}

impl Mailer {
    pub fn new(config: crate::config::SmtpConfig) -> Mailer {
        let mut sender = match config.encryption {
            SmtpEncryption::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host).unwrap(),
            SmtpEncryption::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host).unwrap(),
            SmtpEncryption::Unencrypted => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host),
        };
        if let (Some(u), Some(p)) = (&config.username, &config.password) {
            sender = sender.credentials(Credentials::new(u.to_string(), p.to_string()));
        }
        if let Some(port) = &config.port {
            sender = sender.port(*port)
        }
        Mailer { transport: sender.build(), config }
    }
}

