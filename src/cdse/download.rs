use std::io::Cursor;
use bytes::Bytes;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use reqwest::redirect::Policy;
use zip::ZipArchive;

fn unzip_in_memory(data: Bytes) -> ZipArchive<Cursor<Bytes>> {
    let mut reader = Cursor::new(data);

    reader.set_position(0);

    ZipArchive::new(reader).unwrap()
}

/// This will check if a given file is already stored in the google bucket
async fn google_bucket_check(client: &Client,filename: &str) -> Option<ZipArchive<Cursor<Bytes>>> {
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

    let upload_type = UploadType::Simple(Media::new(filename.to_string()));
    client.upload_object(&UploadObjectRequest {
        bucket: "satellite-storage".to_string(),
        ..Default::default()
    }, file.to_owned(), &upload_type).await.unwrap();
}

/// This will download data and unzip it from ESA. This will be returned as a zipped object
pub async fn download(google_client: &Client,id: &str, token: &str) -> ZipArchive<Cursor<Bytes>> {
    let filename = format!("{id}.zip");

    if let Some(out) = google_bucket_check(google_client,filename.as_str()).await {
        return out;
    }
    
    // create client data to
    let client = reqwest::blocking::Client::builder().redirect(Policy::none()).build().unwrap();
    let mut url = format!("https://catalogue.dataspace.copernicus.eu/odata/v1/Products({})/$value", id);

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