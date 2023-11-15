use std::io::Read;

use reqwest::header;
use serde_json::json;

/// This will pass username and password to cdse and return a api access token
pub fn authenticate(client: &reqwest::blocking::Client, username: &str, password: &str) -> String {
    let request_data = json!({
        "client_id": "cdse-public",
        "username": username,
        "password": password,
        "grant_type": "password",
    });

    let mut buffer = String::new();

    let mut res = client
        .post("https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&request_data)
        .send()
        .unwrap();

    res.read_to_string(&mut buffer).unwrap();

    let response: serde_json::Value = serde_json::from_str(buffer.as_str()).unwrap();

    response["access_token"].as_str().unwrap().to_string()
}