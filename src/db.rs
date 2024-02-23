extern crate google_sheets4 as sheets4;
use sheets4::oauth2::{self, authenticator::Authenticator};
use sheets4::Sheets;
use sheets4::{hyper, hyper_rustls};
use std::collections::HashMap;

pub struct Config {
    pub priv_key: String,
    pub sheet_id: String,
}

impl Config {
    pub fn new() -> Config {
        Config {
            priv_key: String::from("priv_key.json"),
            sheet_id: String::from("1VJI0G67jWe4KFeDyqrUpId1pX1-iK0A16maJ7I_pqP4"),
        }
    }
}

pub async fn auth(
    config: &Config,
    client: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
) -> Authenticator<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
    let secret: oauth2::ServiceAccountKey = oauth2::read_service_account_key(&config.priv_key)
        .await
        .expect("secret not found");

    return oauth2::ServiceAccountAuthenticator::with_client(secret, client.clone())
        .build()
        .await
        .expect("could not create an authenticator");
}


fn main() {
    println!("Hello!");
}
