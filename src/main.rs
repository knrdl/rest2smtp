#[macro_use]
extern crate rocket;

mod mailer;
mod config;

use std::fs;
use std::path::Path;
use std::ffi::OsString;

use rocket::Request;
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::fs::FileServer;
use rocket::http::Status;
use rocket::State;
use rocket::serde::{Deserialize, json::Json};

use lettre::{Message, AsyncTransport};
use lettre::message::{Mailbox, header, MultiPart, SinglePart, Attachment};

#[launch]
fn rocket() -> _ {
    let config = config::SmtpConfig::new();
    config::generate_api_doc().unwrap();
    println!("Running with SMTP Config: host={}, port={}, encryption={}, user={}", config.host,
             match config.port {
                 Some(p) => p.to_string(),
                 None => "(default)".to_string()
             }, config.encryption.to_string(), match &config.username {
            Some(u) => u.to_string(),
            None => "(none)".to_string()
        });
    rocket::build()
        .manage(mailer::Mailer::new(config))
        .mount("/", routes![sendmail_form, sendmail_json])
        .mount("/", FileServer::from("www"))
        .register("/", catchers![not_found, unprocessable_entity, server_error])
}

#[catch(404)]
fn not_found(_req: &Request) -> &'static str {
    "404 not found"
}

#[catch(422)]
fn unprocessable_entity() -> &'static str {
    "422 unprocessable entity"
}

#[catch(500)]
fn server_error() -> &'static str {
    "500 server error"
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
    #[field(validate = len(1..))]
    content_html: String,
    content_text: Option<String>,
}

#[post("/send", format = "multipart/form-data", data = "<params>")]
async fn sendmail_form(params: Form<MailParameterForm<'_>>, mailer: &State<mailer::Mailer>) -> (Status, String) {
    let from_addr = match &params.from_address {
        Some(fa) => fa,
        None => mailer.config.username.as_ref().unwrap()
    };

    let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr.parse().unwrap());

    let mut m = Message::builder().from(from_mailbox).subject(&params.subject);
    for to_address in &params.to_addresses {
        m = m.to(to_address.parse().unwrap());
    }

    let mut multipart = MultiPart::alternative()
        .singlepart(
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
            attachments = attachments.singlepart(Attachment::new(match attachment.name() {
                Some(safe_name) => {
                    let name_no_ext = Path::new(safe_name);
                    let unsafe_name = attachment.raw_name().unwrap().dangerous_unsafe_unsanitized_raw().as_str();
                    let ext_part = Path::new(unsafe_name).extension();
                    let extension = match ext_part {
                        Some(ext) => ext.to_os_string(),
                        None => OsString::from("")
                    };
                    name_no_ext.with_extension(extension).into_os_string().into_string().unwrap()
                }
                None => "attachment".to_string()
            }).body(
                match attachment {
                    TempFile::File { path, .. } => fs::read(path).unwrap(),
                    TempFile::Buffered { content } => content.as_bytes().to_vec()
                },
                match &attachment.content_type() {
                    Some(content_type) => content_type.to_string().parse().unwrap(),
                    None => "application/octet-stream".parse().unwrap()
                },
            ));
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

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct MailParameterJson {
    subject: String,
    from_address: Option<String>,
    from_name: Option<String>,
    to_addresses: Vec<String>,
    content_html: String,
    content_text: Option<String>,
}

#[post("/send", format = "json", data = "<params>")]
async fn sendmail_json(params: Json<MailParameterJson>, mailer: &State<mailer::Mailer>) -> (Status, String) {
    assert!(params.subject.chars().count() >= 1);
    if let Some(fa) = &params.from_address {
        assert!(fa.chars().count() >= 3);
    }
    assert!(params.to_addresses.len() >= 1);
    assert!(params.content_html.chars().count() >= 1);

    let from_addr = match &params.from_address {
        Some(fa) => fa,
        None => mailer.config.username.as_ref().unwrap()
    };

    let from_mailbox = Mailbox::new(params.from_name.clone(), from_addr.parse().unwrap());

    let mut m = Message::builder().from(from_mailbox).subject(&params.subject);
    for to_address in &params.to_addresses {
        m = m.to(to_address.parse().unwrap());
    }

    let mut multipart = MultiPart::alternative()
        .singlepart(
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
