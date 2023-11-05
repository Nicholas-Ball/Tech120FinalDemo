#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use std::fs;
use std::net::IpAddr;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;

use lazy_static::lazy_static;
use opencv::core::{Mat, MatTraitConst, Vector};
use rocket::{Config, post};
use serde::Deserialize;

use crate::cdse::{authenticate, CDSESearch, download, search};
use crate::filters::{false_color, ndwi, swir, true_color};
use crate::sat_data::SatData;

mod sat_data;
pub mod filters;
pub mod cdse;

#[derive(Deserialize)]
struct Keys {
    cdse: CDSEKeys,
}

#[derive(Deserialize)]
struct CDSEKeys {
    username: String,
    password: String,
}

/// This will go and fetch the key file stored on the google bucket
async fn fetch_keys_from_google() -> Keys{
    // authenticate
    let config = ClientConfig::default().with_auth().await.unwrap();
    let client = Client::new(config);

    // download file
    let file = client.download_object(&GetObjectRequest {
        bucket: "satellite-storage".to_string(),
        object: "keys.toml".to_string(),
        ..Default::default()
    }, &Range::default()).await.unwrap();

    // convert to string
    let string_contents = std::str::from_utf8(file.as_slice()).unwrap();

    // return
    toml::from_str(string_contents).unwrap()
}


lazy_static! {
    static ref KEY_FILE: Keys = tokio::runtime::Runtime::new().unwrap().block_on(fetch_keys_from_google());
    static ref TOKEN: String = authenticate(KEY_FILE.cdse.username.as_str(),KEY_FILE.cdse.password.as_str());
}

#[post("/", data = "<input>")]
async fn api_endpoint(input: &str) -> Vec<u8> {
    // convert request to json
    let check: serde_json::error::Result<serde_json::Value> = serde_json::from_str(input);

    // check if request is valid json
    if let Ok(data) = check {

        // create search requirements
        let mut s = CDSESearch {
            satellite: Some("SENTINEL-2".parse().unwrap()),
            geojson: None,
            max_cloud_cover: data["Max Cloud Coverage"].as_f64(),
        };

        // add geojson if present
        if let Ok(test) = serde_json::from_value(data["GeoJson"].clone()) {
            s.geojson = Some(test);
        }

        // collect a list of search results
        let search_results = search(s);

        // default to the latest one
        let id = search_results[0].id.clone();

        // download data
        let zipped = download(id.as_str(), TOKEN.as_str()).await;

        // get sat data
        let sat_data = SatData::new(zipped).unwrap();

        // check filter
        let filter_option = data["Filter"].as_str();

        // check contrast value
        let contrast_option = data["Boost Contrast"].as_f64();

        // if one set, do what they want
        let mut image = if let Some(filter) = filter_option {
            // defualt to true color for check
            if filter == "False Color" {
                false_color(sat_data)
            } else if filter == "NDWI" {
                ndwi(sat_data)
            } else if filter == "SWIR" {
                swir(sat_data)
            } else {
                true_color(sat_data)
            }
        } else {
            true_color(sat_data)
        };

        if let Some(contrast) = contrast_option {
            let mut m = Mat::default();
            image.convert_to(&mut m, -1, contrast, 0.0).unwrap();

            image = m;
        }

        // convert image to png
        let mut buffer = Vector::new();
        opencv::imgcodecs::imencode(".jpg", &image, &mut buffer, &Default::default()).unwrap();

        return buffer.to_vec();
    }

    "Error".to_string().into_bytes()
}

#[launch]
fn rocket() -> _ {
    // get port number
    let port = match std::env::var("PORT") {
        Ok(port) => port,
        _ => String::from("8000")
    };

    let config = Config {
        port: port.parse().unwrap(),
        address: IpAddr::V4("0.0.0.0".parse().unwrap()),
        ..Default::default()
    };

    rocket::custom(config)
        .mount("/", routes![api_endpoint])
}

