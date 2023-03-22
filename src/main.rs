#[macro_use]
extern crate rocket;

mod config;
mod mailer;

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use rocket::form::Form;
use rocket::fs::FileServer;
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::{json::Json, Deserialize};
use rocket::Request;
use rocket::State;

use lettre::message::{header, Attachment, Mailbox, MultiPart, SinglePart};
use lettre::{AsyncTransport, Message};

#[launch]
fn rocket() -> _ {
    let config = config::SmtpConfig::new();
    config::generate_api_doc().unwrap();
    println!(
        "Running with SMTP Config: host={}, port={}, encryption={}, user={}",
        config.host,
        match config.port {
            Some(p) => p.to_string(),
            None => "(default)".to_string(),
        },
        config.encryption.to_string(),
        match &config.username {
            Some(u) => u.to_string(),
            None => "(none)".to_string(),
        }
    );
    rocket::build()
        .manage(mailer::Mailer::new(config))
        .mount("/", routes![sendmail_form, sendmail_json])
        .mount("/", FileServer::from("www"))
        .register(
            "/",
            catchers![
                not_found,
                payload_too_large,
                unprocessable_entity,
                server_error
            ],
        )
}

#[catch(404)]
fn not_found(_req: &Request) -> &'static str {
    "404 not found"
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
fn extract_addrs(addrs: &Vec<String>) -> Vec<String> {
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
    #[field(validate = len(1..))]
    content_html: String,
    content_text: Option<String>,
}

#[post("/send", format = "multipart/form-data", data = "<request_params>")]
async fn sendmail_form(
    request_params: Result<Form<MailParameterForm<'_>>, rocket::form::Errors<'_>>,
    mailer: &State<mailer::Mailer>,
) -> (Status, String) {
    match request_params {
        Ok(params) => {
            let from_addr = match &params.from_address {
                Some(fa) => fa,
                None => mailer.config.username.as_ref().unwrap(),
            };

            let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr.parse().unwrap());

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

            let mut multipart = MultiPart::alternative().singlepart(
                SinglePart::builder()
                    .header(header::ContentType::TEXT_HTML)
                    .body(params.content_html.to_string()),
            );
            if let Some(txt) = &params.content_text {
                multipart = multipart.singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(txt.to_string()),
                )
            }

            let mail_body = if params.attachments.len() > 0 {
                let mut attachments = MultiPart::mixed().multipart(multipart);
                for attachment in &params.attachments {
                    attachments = attachments.singlepart(
                        Attachment::new(match attachment.name() {
                            Some(safe_name) => {
                                let name_no_ext = Path::new(safe_name);
                                let unsafe_name = attachment
                                    .raw_name()
                                    .unwrap()
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
                                    .unwrap()
                            }
                            None => "attachment".to_string(),
                        })
                        .body(
                            match attachment {
                                TempFile::File { path, .. } => fs::read(path).unwrap(),
                                TempFile::Buffered { content } => content.as_bytes().to_vec(),
                            },
                            match &attachment.content_type() {
                                Some(content_type) => content_type.to_string().parse().unwrap(),
                                None => "application/octet-stream".parse().unwrap(),
                            },
                        ),
                    );
                }
                attachments
            } else {
                multipart
            };

            let mail: Message = m.multipart(mail_body).unwrap();
            match mailer.transport.send(mail).await {
                Ok(x) => (Status::Ok, x.first_line().unwrap().to_string()),
                Err(e) => (Status::InternalServerError, e.to_string()),
            }
        }
        Err(errors) => {
            let err_text = errors
                .iter()
                .map(|err| format!("{:?}", err))
                .collect::<Vec<_>>()
                .join("\n");
            (Status::UnprocessableEntity, err_text.into())
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
    content_html: String,
    content_text: Option<String>,
}

#[post("/send", format = "json", data = "<request_params>")]
async fn sendmail_json(
    request_params: Result<Json<MailParameterJson>, rocket::serde::json::Error<'_>>,
    mailer: &State<mailer::Mailer>,
) -> (Status, String) {
    match request_params {
        Ok(params) => {
            // manual data validation required, https://github.com/SergioBenitez/Rocket/issues/1915
            if params.subject.chars().count() == 0 {
                return (
                    Status::UnprocessableEntity,
                    "subject missing or empty".into(),
                );
            }
            if let Some(fa) = &params.from_address {
                if fa.chars().count() < 3 {
                    return (Status::UnprocessableEntity, "from_address invalid".into());
                }
            }
            if params.to_addresses.len() == 0 {
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
            if params.content_html.chars().count() == 0 {
                return (
                    Status::UnprocessableEntity,
                    "content_html missing or empty".into(),
                );
            }

            let from_addr = match &params.from_address {
                Some(fa) => fa,
                None => mailer.config.username.as_ref().unwrap(),
            };

            let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr.parse().unwrap());

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

            let mut multipart = MultiPart::alternative().singlepart(
                SinglePart::builder()
                    .header(header::ContentType::TEXT_HTML)
                    .body(params.content_html.to_string()),
            );
            if let Some(txt) = &params.content_text {
                multipart = multipart.singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(txt.to_string()),
                )
            }

            let mail: Message = m.multipart(multipart).unwrap();

            match mailer.transport.send(mail).await {
                Ok(x) => (Status::Ok, x.first_line().unwrap().to_string()),
                Err(e) => (Status::InternalServerError, e.to_string()),
            }
        }
        Err(e) => (Status::UnprocessableEntity, format!("{}", e).into()),
    }
}
