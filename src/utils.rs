use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use log::info;
use log::error;
use log::warn;

pub async fn send_email(name: &str, to: &str, from: &str, msg: &str, server: &str, port: u16, username: &Option<String>, password: &Option<String>)  {

    info!("send_email called...");
    	let email = Message::builder()
    	.from(from.parse().unwrap())
    	.reply_to(to.parse().unwrap())
    	.to(to.parse().unwrap())
    	.subject("Alert: Pod Status Not Running")
    	.header(ContentType::TEXT_PLAIN)
    	.body(msg.to_string())
    	.unwrap();
	
	let mailer = match (username, password) {

		(Some(user), Some(pass)) => {let creds = Credentials::new("smtp_username".to_owned(), "smtp_password".to_owned());
				             SmtpTransport::builder_dangerous(server).port(port).credentials(creds).build()},
                (None, None) => { warn!("Connecting to smtp without credentials for: {:?}", name);
                                  SmtpTransport::builder_dangerous(server).port(port).build()}

                _ => { error!("Username and password both are required for provided for smtp server: {:?}!!!", name);
                       return () }, 
	};

	// Send the email
	match mailer.send(&email) {
    	Ok(_) => info!("Email sent successfully!"),
    	Err(e) => error!("Could not send email: {e:?}"),
	}
 
}
