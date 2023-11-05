use std::io::{Cursor, Read};

use bytes::Bytes;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use reqwest::header;
use reqwest::redirect::Policy;
use serde_json::json;
use zip::ZipArchive;

use crate::cdse::search_result::*;

pub mod search_result;

#[derive(Debug)]
pub struct CDSESearch {
    pub satellite: Option<String>,
    pub geojson: Option<serde_json::Value>,
    pub max_cloud_cover: Option<f64>,
}

fn parse_geojson_to_odata(json: serde_json::Value) -> String {
    // get intersect type
    let t = json["features"][0]["geometry"]["type"].as_str().unwrap();

    // get coordinates
    let coords = json["features"][0]["geometry"]["coordinates"][0].as_array().unwrap();

    // build string
    let mut to_return = t.to_uppercase() + "((";

    for x in coords {
        to_return.push_str((x[0].as_f64().unwrap().to_string() + " " + x[1].as_f64().unwrap().to_string().as_str() + ",").as_str());
    }

    // remove last comma
    to_return = to_return[..to_return.len() - 1].to_string();

    // add odata ending
    to_return.push_str("))");

    to_return
}

fn unzip_in_memory(data: Bytes) -> ZipArchive<Cursor<Bytes>> {
    let mut reader = Cursor::new(data);

    reader.set_position(0);

    ZipArchive::new(reader).unwrap()
}

/// This will pass username and password to cdse and return a api access token
pub fn authenticate(username: &str, password: &str) -> String {
    let request_data = json!({
        "client_id": "cdse-public",
        "username": username,
        "password": password,
        "grant_type": "password",
    });

    let client = reqwest::blocking::Client::new();
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

/// Given certain search criteria, we can filter what data we see
pub fn search(cdsesearch: CDSESearch) -> Vec<SearchResult> {
    // create client data to
    let client = reqwest::blocking::Client::new();
    let mut buffer = String::new();

    // build request url
    let mut url = "https://catalogue.dataspace.copernicus.eu/odata/v1/Products?$filter=".to_string();

    if cdsesearch.satellite.is_some() {
        url.push_str(format!("Collection/Name eq '{}' and ", cdsesearch.satellite.unwrap()).as_str());
    }

    if cdsesearch.geojson.is_some() {
        let geojsoned = parse_geojson_to_odata(cdsesearch.geojson.unwrap());

        url.push_str(format!("OData.CSC.Intersects(area=geography'SRID=4326;{}') and ", geojsoned).as_str());
    }

    if cdsesearch.max_cloud_cover.is_some() {
        url.push_str(format!("Attributes/OData.CSC.DoubleAttribute/any(att:att/Name eq 'cloudCover' and att/OData.CSC.DoubleAttribute/Value le {}) and ", cdsesearch.max_cloud_cover.unwrap()).as_str());
    }

    // remove and at the end
    url = url[0..url.len() - 5].to_string();

    // add that we want to sort newest to oldest
    url.push_str("&$orderby=ContentDate/Start desc");

    let mut res = client
        .get(url)
        .send()
        .unwrap();

    res.read_to_string(&mut buffer).unwrap();

    let to_return: serde_json::Value = serde_json::from_str(buffer.as_str()).unwrap();

    parse_search_result(to_return.get("value").unwrap().clone())
}

/// This will check if a given file is already stored in the google bucket
async fn google_bucket_check(filename: &str) -> Option<ZipArchive<Cursor<Bytes>>> {
    // authenticate
    let config = ClientConfig::default().with_auth().await.unwrap();
    let client = Client::new(config);

    // check for file
    let data = client.download_object(&GetObjectRequest {
        bucket: "satellite-storage".to_string(),
        object: filename.to_string(),
        ..Default::default()
    }, &Range::default()).await;

    if let Ok(file) = data {
        Some(unzip_in_memory(Bytes::from(file)))
    } else {
        None
    }
}

/// This will upload a zip file to google bucket
async fn google_bucket_upload_zip(filename: &str, file: &Bytes) {
    // authenticate
    let config = ClientConfig::default().with_auth().await.unwrap();
    let client = Client::new(config);

    let upload_type = UploadType::Simple(Media::new(filename.clone().to_string()));
    let uploaded = client.upload_object(&UploadObjectRequest {
        bucket: "satellite-storage".to_string(),
        ..Default::default()
    }, file.clone(), &upload_type).await.unwrap();
}

/// This will download data and unzip it from ESA. This will be returned as a zipped object
pub async fn download(id: &str, token: &str) -> ZipArchive<Cursor<Bytes>> {

    // create client data to
    let client = reqwest::blocking::Client::builder().redirect(Policy::none()).build().unwrap();
    let mut url = format!("https://catalogue.dataspace.copernicus.eu/odata/v1/Products({})/$value", id);

    let filename = format!("{id}.zip");

    if let Some(out) = google_bucket_check(filename.as_str()).await {
        return out;
    }

    // get initial request
    let mut resp = client
        .get(url.as_str())
        .bearer_auth(token)
        .send()
        .unwrap();

    // follow redirects
    let redirect_codes = [301, 302, 303, 307];
    while redirect_codes.contains(&resp.status().as_u16()) {
        let wtf_header = resp.headers();

        url = wtf_header["location"].to_str().unwrap().to_string();

        resp = client
            .get(url.as_str())
            .bearer_auth(token)
            .send()
            .unwrap();
    }

    let client_with_redirect = reqwest::blocking::Client::builder().timeout(None).build().unwrap();

    resp = client_with_redirect
        .get(url.as_str())
        .bearer_auth(token)
        .send()
        .unwrap();

    let data = resp.bytes().unwrap();

    // upload copy
    google_bucket_upload_zip(filename.as_str(), &data).await;

    unzip_in_memory(data)
}