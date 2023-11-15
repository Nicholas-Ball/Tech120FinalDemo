#![feature(decl_macro)]
#![feature(async_closure)]

#[macro_use]
extern crate rocket;

use std::io::Read;
use std::net::IpAddr;
use std::thread::spawn;

use base64::Engine;
use base64::engine::general_purpose;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use lazy_static::lazy_static;
use opencv::core::{Mat, MatTraitConst, Vector};
use rocket::{Config, post};
use serde::{Deserialize, Serialize};
use xz2::read::XzEncoder;

use crate::cdse::CDSE;
use crate::cdse::search::{CDSESearch, search};

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

#[derive(Serialize)]
struct ImageReturn {
    id: String,
    image: String,
}

#[derive(Serialize)]
struct ImageReturnV2 {
    id: String,
}

/// This will go and fetch the key file stored on the google bucket
async fn fetch_keys_from_google() -> Keys {
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
    static ref CDSE_Instance: CDSE = tokio::runtime::Runtime::new().unwrap().block_on(cdse::CDSE::new(KEY_FILE.cdse.username.as_str(),KEY_FILE.cdse.password.as_str()));
}

/// Gzip compress data
fn compress(data: &[u8]) -> Vec<u8> {
    let mut c = Vec::new();
    let mut compressed = XzEncoder::new(data, 9);
    compressed.read_to_end(&mut c).unwrap();

    c
}

pub fn compress_and_encode(image: &[u8]) -> String {
    let c = compress(image);

    general_purpose::STANDARD.encode(c)
}

fn handle_image_return(data: &serde_json::Value, id: &str) -> String {
    // check filter
    let filter_option = data["Filter"].clone();

    let id_string = id.to_string();

    // if one set, do what they want
    let image_await = spawn(move || {
        if let Some(filter) = filter_option.as_str() {
            tokio::runtime::Runtime::new().unwrap().block_on(CDSE_Instance.fetch(id_string.as_str(), filter))
        } else {
            tokio::runtime::Runtime::new().unwrap().block_on(CDSE_Instance.fetch(id_string.as_str(), "True Color"))
        }
    }).join();

    let mut image = image_await.unwrap();

    // check contrast value
    let contrast_option = data["Boost Contrast"].as_f64();

    if let Some(contrast) = contrast_option {
        if contrast != 1.0 {
            let conv: Vector<u8> = Vector::from(image);

            let image_mat = opencv::imgcodecs::imdecode(&conv as _, opencv::imgcodecs::IMREAD_COLOR).unwrap();

            let mut m = Mat::default();
            image_mat.convert_to(&mut m, -1, contrast, 0.0).unwrap();

            let mut wtf = Vector::new();

            opencv::imgcodecs::imencode(".jpg", &m, &mut wtf, &Default::default()).unwrap();

            image = wtf.to_vec();
        }
    }

    return compress_and_encode(image.as_slice());
}

fn handle_image_return_v2(id: &str, filter: &str, contrast:f32) -> Vec<u8> {

    let id_string = id.to_string();

    let filter_clone = filter.to_string();

    // if one set, do what they want
    let image_await = spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(CDSE_Instance.fetch(id_string.as_str(), filter_clone.as_str()))
    }).join();

    let mut image = image_await.unwrap();

    // check contrast value
    let conv: Vector<u8> = Vector::from(image);

    let image_mat = opencv::imgcodecs::imdecode(&conv as _, opencv::imgcodecs::IMREAD_COLOR).unwrap();

    let mut m = Mat::default();
    dbg!(contrast as f64);
    image_mat.convert_to(&mut m, -1, contrast as f64, 0.0).unwrap();

    let mut wtf = Vector::new();

    opencv::imgcodecs::imencode(".jpg", &m, &mut wtf, &Default::default()).unwrap();

    image = wtf.to_vec();

    return compress(image.as_slice());
}

fn search_with_json(data: &serde_json::Value) -> String {
    let s = parse_to_search(data);

    // collect a list of search results
    let search_results = search(s);

    // default to the latest one
    search_results[0].id.clone()
}

fn parse_to_search(data: &serde_json::Value) -> CDSESearch {
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

    s
}


/// This will only fetch new images from ESA
#[post("/v2", data = "<input>")]
async fn api_v2_endpoint(input: &str) -> Vec<u8> {
    // there are two commands here, new and change. New will get and fetch an image with search
    // criteria and change will get an already existing image out of storage

    let to_json: serde_json::error::Result<serde_json::Value> = serde_json::from_str(input);
    let error: Vec<u8> = (serde_json::from_str("{\"Result\":\"Error\"}") as serde_json::error::Result<serde_json::Value>).unwrap().to_string().into_bytes();

    if let Ok(json) = to_json {
        // default to new search
        let id = search_with_json(&json);

        let to_return = ImageReturnV2 { id };

        serde_json::to_vec(&to_return).unwrap()
    } else {
        error
    }
}

/// THis will fetch image from storage
#[get("/v2/fetch?<id>&<filter>&<contrast>")]
async fn api_v2_fetch(id: &str,filter: &str, contrast:f32) -> Vec<u8> {
    handle_image_return_v2(id,filter,contrast)
}

#[post("/v1", data = "<input>")]
async fn api_v1_endpoint(input: &str) -> Vec<u8> {
    // there are two commands here, new and change. New will get and fetch an image with search
    // criteria and change will get an already existing image out of storage

    let to_json: serde_json::error::Result<serde_json::Value> = serde_json::from_str(input);
    let error: Vec<u8> = (serde_json::from_str("{\"Result\":\"Error\"}") as serde_json::error::Result<serde_json::Value>).unwrap().to_string().into_bytes();


    if let Ok(json) = to_json {
        if let Some(command) = json["Command"].as_str() {
            if command == "Change" {
                // check if ID is set
                if let Some(id) = json["ID"].as_str() {
                    let image = handle_image_return(&json, id);

                    let to_return = ImageReturn { id: id.to_string(), image };

                    serde_json::to_vec(&to_return).unwrap()
                } else {
                    error
                }
            } else {
                // default to new search
                let id = search_with_json(&json);

                let image = handle_image_return(&json, id.as_str());

                let to_return = ImageReturn { id, image };

                serde_json::to_vec(&to_return).unwrap()
            }
        } else {
            error
        }
    } else {
        error
    }
}


#[post("/", data = "<input>")]
async fn api_endpoint(input: &str) -> Vec<u8> {
    // convert request to json
    let check: serde_json::error::Result<serde_json::Value> = serde_json::from_str(input);

    // check if request is valid json
    if let Ok(data) = check {
        let id = search_with_json(&data);

        return handle_image_return(&data, id.as_str()).into_bytes();
    }

    "Error".to_string().into_bytes()
}

#[launch]
fn rocket() -> _ {
    println!("{}", KEY_FILE.cdse.username);

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
        .mount("/", routes![api_endpoint, api_v1_endpoint, api_v2_endpoint, api_v2_fetch])
}

