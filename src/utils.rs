use lettre::message::header::ContentType;
// use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::Tls;
use lettre::transport::smtp::client::TlsParameters;
use lettre::{Message, SmtpTransport, Transport};
use log::error;
use log::info;
use log::warn;
use reqwest;
use serde_json::json;
use crate::crd::TlsOption;

pub async fn send_email(
    name: &str,
    to: &str,
    from: &str,
    msg: &str,
    server: &str,
    port: u16,
    tls: &Option<&TlsOption>,
    username: &Option<&String>,
    password: &Option<&String>,
) {
    info!("send_email called...");
    let email = Message::builder()
        .from(from.parse().unwrap())
        .reply_to(to.parse().unwrap())
        .to(to.parse().unwrap())
        .subject("Alert: Pod Status Not Running")
        .header(ContentType::TEXT_PLAIN)
        .body(msg.to_string())
        .unwrap();

    let mailer = match (*username, *password) {
        (Some(user), Some(pass)) => {
            println!("{}", user.to_string());
            println!("{}", pass.to_string());
            let creds = Credentials::new(user.to_string(), pass.to_string());

            if tls.unwrap_or(&TlsOption::False).as_str() == "false" {
            SmtpTransport::builder_dangerous(server)
                .port(port)
                .credentials(creds)
                .authentication(vec![Mechanism::Plain, Mechanism::Login])
                .build()
            }
            else {

            let tls_parameters = TlsParameters::builder(server.to_string())
            .dangerous_accept_invalid_certs(true)
            .build();

            SmtpTransport::relay(server).expect("REASON")
            .port(port)
            .tls(Tls::Required(tls_parameters.expect("REASON")))
            .credentials(creds)
            .build()
            }
        }
        (None, None) => {
            warn!("Connecting to smtp without credentials for: {:?}", name);
            SmtpTransport::builder_dangerous(server).port(port).build()
        }

        _ => {
            error!(
                "Username and password both are required for provided for smtp server: {:?}!!!",
                name
            );
            return ();
        }
    };

    // Send the email
    match mailer.send(&email) {
        Ok(_) => info!("Email sent successfully!"),
        Err(e) => error!("Could not send email: {e:?}"),
    }
}

pub async fn post_data(
    url: &String,
    data: serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.post(url).json(&data).send().await?.text().await?;

    Ok(response)
}
