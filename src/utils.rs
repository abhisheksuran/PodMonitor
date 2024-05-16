use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

pub async fn send_email(to: &str, from: &str, msg: &str, server: &str, port: u16)  {

    println!("send_email called...");
    	let email = Message::builder()
    	.from(from.parse().unwrap())
    	.reply_to(to.parse().unwrap())
    	.to(to.parse().unwrap())
    	.subject("Alert: Pod Status Not Running")
    	.header(ContentType::TEXT_PLAIN)
    	.body(msg.to_string())
    	.unwrap();

	//let creds = Credentials::new("smtp_username".to_owned(), "smtp_password".to_owned());

	// Open a remote connection to gmail
	let mailer = SmtpTransport::builder_dangerous(server)
        .port(port)
    	.build();
    	//.credentials(creds)
    	//.build();

	// Send the email
	match mailer.send(&email) {
    	Ok(_) => println!("Email sent successfully!"),
    	Err(e) => panic!("Could not send email: {e:?}"),
	}

   // println!("{:?}", result); 
}
