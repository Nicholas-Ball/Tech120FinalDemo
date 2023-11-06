#![feature(decl_macro)]
#![feature(async_closure)]

#[macro_use]
extern crate rocket;

use std::io::Cursor;
use std::net::IpAddr;
use std::thread::spawn;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use image::codecs::jpeg::JpegEncoder;

use lazy_static::lazy_static;
use opencv::core::{Mat, MatTraitConst, Vector};
use rocket::{Config, post};
use serde::Deserialize;

use crate::cdse::search::{CDSESearch, search};
use crate::cdse::CDSE;

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
    static ref CDSE_Instance: CDSE = tokio::runtime::Runtime::new().unwrap().block_on(cdse::CDSE::new(KEY_FILE.cdse.username.as_str(),KEY_FILE.cdse.password.as_str()));
}

pub fn compress_till_32MB(image:&[u8])->Vec<u8>{
    let img = image::load_from_memory(image).unwrap();
    let mut quality = 95;
    let max_size: u64 = 32 * 1024 * 1024; // 32MB

    loop {
        let mut buffer = Cursor::new(Vec::new());

        // Encode the image to JPEG with the current quality setting
        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
        encoder.encode(img.as_rgb8().unwrap(), img.width(), img.height(), image::ColorType::Rgb8).unwrap();

        // Get the resulting JPEG data
        let data = buffer.into_inner();

        if data.len() as u64 <= max_size {
            // write the compressed image to the output
            return data
        } else if quality > 10 { // reduce the quality by 10 if the size is still too big
            quality -= 10;
        } else { // if quality less than 10 then break the loop
            println!("Could not compress image to desired size");
            return data
        }
    }
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
        let mut id = search_results[0].id.clone();

        // // try to find whole image data
        // for x in search_results{
        //     if x.online && x.num_points > 5 && x.file_size > 600_000_000{
        //         id = x.id;
        //         break
        //     }
        // }

        // check filter
        let filter_option = data["Filter"].clone();

        // if one set, do what they want
        let image_await = spawn(move || {
            if let Some(filter) = filter_option.as_str() {
                tokio::runtime::Runtime::new().unwrap().block_on(CDSE_Instance.fetch(id.as_str(), filter))

            } else {
                tokio::runtime::Runtime::new().unwrap().block_on(CDSE_Instance.fetch(id.as_str(), "True Color"))
            }
        }).join();

        let mut image = image_await.unwrap();

        // check contrast value
        let contrast_option = data["Boost Contrast"].as_f64();

        if let Some(contrast) = contrast_option {

            if contrast != 1.0{
                let conv: Vector<u8> = Vector::from(image);

                let image_mat = opencv::imgcodecs::imdecode(&conv as _,opencv::imgcodecs::IMREAD_COLOR).unwrap();

                let mut m = Mat::default();
                image_mat.convert_to(&mut m, -1, contrast, 0.0).unwrap();

                let mut wtf = Vector::new();

                opencv::imgcodecs::imencode(".jpg",&m,&mut wtf,&Default::default()).unwrap();

                image = wtf.to_vec();
            }
        }

        return compress_till_32MB(image.as_slice())
    }

    "Error".to_string().into_bytes()
}

#[launch]
fn rocket() -> _ {

    println!("{}",KEY_FILE.cdse.username);

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

