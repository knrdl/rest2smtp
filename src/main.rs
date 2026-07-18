#[macro_use]
extern crate rocket;

mod auth;
mod config;
mod mailer;
mod swagger;

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use rocket::{
    form::Form,
    fs::{FileServer, TempFile},
    http::Status,
    serde::{json::Json, Deserialize},
    Request, State,
};

use lettre::{
    message::{Attachment, Mailbox, MultiPart, SinglePart},
    Address,
};
use lettre::{AsyncTransport, Message};

use auth::{ApiAuth, ApiTokenConfig};

#[rocket::main]
async fn main() -> Result<(), Box<rocket::Error>> {
    let config = config::SmtpConfig::new();
    let api_token = ApiTokenConfig::from_env();
    swagger::generate_api_doc(api_token.enabled()).unwrap();
    println!(
        "Running with SMTP Config: host={}, port={}, encryption={}, user={}, api_auth={}",
        config.host,
        match config.port {
            Some(p) => p.to_string(),
            None => "(default)".to_string(),
        },
        config.encryption,
        match &config.username {
            Some(u) => u.to_string(),
            None => "(none)".to_string(),
        },
        if api_token.enabled() {
            "enabled"
        } else {
            "disabled"
        }
    );
    let _rocket = rocket::build()
        .manage(mailer::Mailer::new(config))
        .mount("/", routes![sendmail_form, sendmail_json])
        .mount("/", FileServer::from("www"))
        .register(
            "/",
            catchers![
                not_found,
                unauthorized,
                payload_too_large,
                unprocessable_entity,
                server_error
            ],
        )
        .launch()
        .await?;

    Ok(())
}

#[catch(404)]
fn not_found(_req: &Request) -> &'static str {
    "404 not found"
}

struct Unauthorized;

impl<'r> rocket::response::Responder<'r, 'static> for Unauthorized {
    fn respond_to(self, _: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        Ok(rocket::Response::build()
            .status(rocket::http::Status::Unauthorized)
            .header(rocket::http::Header::new(
                "WWW-Authenticate",
                r#"Bearer realm="api""#,
            ))
            .sized_body(
                "401 unauthorized".len(),
                std::io::Cursor::new("401 unauthorized"),
            )
            .finalize())
    }
}

#[catch(401)]
fn unauthorized(_: &rocket::Request<'_>) -> Unauthorized {
    Unauthorized
}

#[catch(413)]
fn payload_too_large(_req: &Request) -> &'static str {
    "413 payload too large"
}

#[catch(422)]
fn unprocessable_entity(_req: &Request) -> &'static str {
    "422 unprocessable entity"
}

#[catch(500)]
fn server_error() -> &'static str {
    "500 server error"
}

// the form data might contain addresses in the form "mail1@example.org,mail2@example.org" instead of ["mail1@example.org","mail2@example.org"]
fn extract_addrs(addrs: &[String]) -> Vec<String> {
    if addrs.len() == 1 && addrs[0].contains(",") {
        addrs[0]
            .split(",")
            .map(|addr| addr.trim().to_string())
            .filter(|addr| !addr.is_empty())
            .collect()
    } else {
        addrs.to_vec()
    }
}

fn find_from_addr(
    request_value: &Option<String>,
    mailer: &mailer::Mailer,
) -> Result<Address, (Status, String)> {
    let from_addr = match &request_value {
        Some(fa) => Some(fa),
        None => mailer.config.username.as_ref(),
    };

    if let Some(from_addr) = from_addr {
        if let Ok(from_addr) = from_addr.parse::<Address>() {
            Ok(from_addr)
        } else {
            Err((Status::UnprocessableEntity, "from_address invalid".into()))
        }
    } else {
        Err((
            Status::UnprocessableEntity,
            "from_address missing and no default configured".into(),
        ))
    }
}

#[derive(FromForm)]
struct MailParameterForm<'r> {
    #[field(validate = len(1..))]
    subject: String,
    #[field(name = "attachment")]
    attachments: Vec<TempFile<'r>>,
    from_address: Option<String>,
    from_name: Option<String>,
    #[field(validate = len(1..), name = "to_address")]
    to_addresses: Vec<String>,
    #[field(name = "cc_address")]
    cc_addresses: Vec<String>,
    #[field(name = "bcc_address")]
    bcc_addresses: Vec<String>,
    content_html: Option<String>,
    content_text: Option<String>,
}

#[post("/send", format = "multipart/form-data", data = "<request_params>")]
async fn sendmail_form(
    _auth: ApiAuth,
    request_params: Result<Form<MailParameterForm<'_>>, rocket::form::Errors<'_>>,
    mailer: &State<mailer::Mailer>,
) -> (Status, String) {
    match request_params {
        Ok(params) => {
            let from_addr = match find_from_addr(&params.from_address, mailer) {
                Ok(addr) => addr,
                Err((status, msg)) => return (status, msg),
            };

            let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr);

            let mut m = Message::builder()
                .from(from_mailbox)
                .subject(&params.subject);
            for to_address in extract_addrs(&params.to_addresses) {
                if let Ok(addr) = to_address.parse() {
                    m = m.to(addr);
                } else {
                    return (Status::UnprocessableEntity, "invalid to_address".into());
                }
            }
            for cc_address in extract_addrs(&params.cc_addresses) {
                if let Ok(addr) = cc_address.parse() {
                    m = m.cc(addr);
                } else {
                    return (Status::UnprocessableEntity, "invalid cc_address".into());
                }
            }
            for bcc_address in extract_addrs(&params.bcc_addresses) {
                if let Ok(addr) = bcc_address.parse() {
                    m = m.bcc(addr);
                } else {
                    return (Status::UnprocessableEntity, "invalid bcc_address".into());
                }
            }

            let multipart = match (&params.content_text, &params.content_html) {
                (Some(txt), Some(html)) => MultiPart::alternative()
                    .singlepart(SinglePart::plain(txt.clone()))
                    .singlepart(SinglePart::html(html.clone())),

                (Some(txt), None) => {
                    MultiPart::alternative().singlepart(SinglePart::plain(txt.clone()))
                }

                (None, Some(html)) => {
                    MultiPart::alternative().singlepart(SinglePart::html(html.clone()))
                }

                (None, None) => MultiPart::alternative().build(),
            };
            let mail_body = if !params.attachments.is_empty() {
                let mut attachments = MultiPart::mixed().multipart(multipart);
                for attachment in &params.attachments {
                    attachments = attachments.singlepart(
                        Attachment::new(match attachment.name() {
                            Some(safe_name) => {
                                let name_no_ext = Path::new(safe_name);
                                let unsafe_name = attachment
                                    .raw_name()
                                    .unwrap_or("".into())
                                    .dangerous_unsafe_unsanitized_raw()
                                    .as_str();
                                let ext_part = Path::new(unsafe_name).extension();
                                let extension = match ext_part {
                                    Some(ext) => ext.to_os_string(),
                                    None => OsString::from(""),
                                };
                                name_no_ext
                                    .with_extension(extension)
                                    .into_os_string()
                                    .into_string()
                                    .unwrap_or(safe_name.into())
                            }
                            None => "attachment".to_string(),
                        })
                        .body(
                            match attachment {
                                TempFile::File { path, .. } => fs::read(path).unwrap(),
                                TempFile::Buffered { content } => content.to_vec(),
                            },
                            match &attachment.content_type() {
                                Some(content_type) => {
                                    content_type.to_string().parse().unwrap_or_else(|_| {
                                        "application/octet-stream".parse().unwrap()
                                    })
                                }
                                None => "application/octet-stream".parse().unwrap(),
                            },
                        ),
                    );
                }
                attachments
            } else {
                multipart
            };

            match m.multipart(mail_body) {
                Ok(mail) => match mailer.transport.send(mail).await {
                    Ok(x) => (Status::Ok, x.first_line().unwrap_or("").to_string()),
                    Err(e) => (Status::InternalServerError, e.to_string()),
                },
                Err(e) => (Status::InternalServerError, e.to_string()),
            }
        }
        Err(errors) => {
            let err_text = errors
                .iter()
                .map(|err| format!("{:?}", err))
                .collect::<Vec<_>>()
                .join("\n");
            (Status::UnprocessableEntity, err_text)
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
struct MailParameterJson {
    subject: String,
    from_address: Option<String>,
    from_name: Option<String>,
    to_addresses: Vec<String>,
    cc_addresses: Option<Vec<String>>,
    bcc_addresses: Option<Vec<String>>,
    content_html: Option<String>,
    content_text: Option<String>,
}

#[post("/send", format = "json", data = "<request_params>")]
async fn sendmail_json(
    _auth: ApiAuth,
    request_params: Result<Json<MailParameterJson>, rocket::serde::json::Error<'_>>,
    mailer: &State<mailer::Mailer>,
) -> (Status, String) {
    match request_params {
        Ok(params) => {
            // manual data validation required, https://github.com/SergioBenitez/Rocket/issues/1915
            if params.subject.is_empty() {
                return (
                    Status::UnprocessableEntity,
                    "subject missing or empty".into(),
                );
            }

            let from_addr = match find_from_addr(&params.from_address, mailer) {
                Ok(addr) => addr,
                Err((status, msg)) => return (status, msg),
            };
            if params.to_addresses.is_empty() {
                return (
                    Status::UnprocessableEntity,
                    "to_addresses missing or empty".into(),
                );
            } else {
                for to_addr in &params.to_addresses {
                    if to_addr.chars().count() < 3 {
                        return (
                            Status::UnprocessableEntity,
                            "to_addresses contains invalid address".into(),
                        );
                    }
                }
            }

            let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr);

            let mut m = Message::builder()
                .from(from_mailbox)
                .subject(&params.subject);
            for to_address in &params.to_addresses {
                if let Ok(addr) = to_address.parse() {
                    m = m.to(addr);
                } else {
                    return (Status::UnprocessableEntity, "invalid to_address".into());
                }
            }
            if let Some(cc_addrs) = &params.cc_addresses {
                for cc_address in cc_addrs {
                    if let Ok(addr) = cc_address.parse() {
                        m = m.cc(addr);
                    } else {
                        return (Status::UnprocessableEntity, "invalid cc_address".into());
                    }
                }
            }
            if let Some(bcc_addrs) = &params.bcc_addresses {
                for bcc_address in bcc_addrs {
                    if let Ok(addr) = bcc_address.parse() {
                        m = m.bcc(addr);
                    } else {
                        return (Status::UnprocessableEntity, "invalid bcc_address".into());
                    }
                }
            }

            let multipart = match (&params.content_text, &params.content_html) {
                (Some(txt), Some(html)) => MultiPart::alternative()
                    .singlepart(SinglePart::plain(txt.clone()))
                    .singlepart(SinglePart::html(html.clone())),

                (Some(txt), None) => {
                    MultiPart::alternative().singlepart(SinglePart::plain(txt.clone()))
                }

                (None, Some(html)) => {
                    MultiPart::alternative().singlepart(SinglePart::html(html.clone()))
                }

                (None, None) => MultiPart::alternative().build(),
            };

            match m.multipart(multipart) {
                Ok(mail) => match mailer.transport.send(mail).await {
                    Ok(x) => (Status::Ok, x.first_line().unwrap_or("").to_string()),
                    Err(e) => (Status::InternalServerError, e.to_string()),
                },
                Err(e) => (Status::InternalServerError, e.to_string()),
            }
        }
        Err(e) => (Status::UnprocessableEntity, format!("{}", e)),
    }
}
