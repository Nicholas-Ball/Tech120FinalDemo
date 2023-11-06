use std::thread;
use bytes::Bytes;
use google_cloud_storage::client::ClientConfig;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use opencv::core::{Mat, Vector};
use reqwest::blocking::Client;
use tokio::runtime::Runtime;
use crate::filters::{false_color, ndwi, swir, true_color};
use crate::sat_data::SatData;

pub mod search_result;
mod download;
pub mod search;
mod authenticate;

async fn upload_image_to_bucket(client: &google_cloud_storage::client::Client, id: &str,filter: &str, sat_data: &SatData){

    // default to true color for check
    let image = if filter == "False Color" {
        false_color(&sat_data)
    } else if filter == "NDWI" {
        ndwi(&sat_data)
    } else if filter == "SWIR" {
        swir(&sat_data)
    } else {
        true_color(&sat_data)
    };

    // prepare image
    let dir = id.to_owned() + "/" + filter + ".jpg";
    let mut image_bytes = Vector::new();
    opencv::imgcodecs::imencode(".jpg",&image, &mut image_bytes, &Default::default()).unwrap();

    let upload_type = UploadType::Simple(Media::new(dir));

    client.upload_object(&UploadObjectRequest {
        bucket: "satellite-storage".to_string(),
        ..Default::default()
    },  Bytes::from(image_bytes.to_vec()), &upload_type).await.unwrap();
}

pub struct CDSE{
    cdse_client: Client,
    google_client: google_cloud_storage::client::Client,
    token: String,
}

impl CDSE {
    /// Create new CDSE instance
    pub async fn new(username: &str, password: &str) -> CDSE {
        // create clients
        let cdse_client = Client::new();

        // authenticate
        let config = ClientConfig::default().with_auth().await.unwrap();
        let google_client = google_cloud_storage::client::Client::new(config);

        // get access token
        let token = authenticate::authenticate(&cdse_client, username, password);

        CDSE {
            cdse_client,
            google_client,
            token
        }
    }

    async fn check_bucket_and_download(&self, filename: &str) -> Option<Bytes> {
        let data_result = self.google_client.download_object(&GetObjectRequest {
            bucket: "satellite-storage".to_string(),
            object: filename.to_string(),
            ..Default::default()
        }, &Range::default()).await;

        if let Ok(data) = data_result {
            Some(Bytes::from(data))
        } else {
            None
        }
    }

    /// Return a image from an ID with a given filter and contrast
    pub async fn fetch(&self, id: &str, filter: &str) -> Vec<u8> {

        let dir = id.to_owned() + "/" + filter + ".jpg";

        // check if filter exists
        let image_with_filter_result = self.check_bucket_and_download(dir.as_str()).await;

        // if image with filter does exist, return it
        if let Some(image) = image_with_filter_result {
            image.to_vec()
        } else {
            // check if zip exits. if not, download
            let zip = download::download(&self.google_client, id, self.token.as_str()).await;

            // load to sat data
            let sat_data = SatData::new(zip).unwrap();

            // default to true color for check
            let m = if filter == "False Color" {
                false_color(&sat_data)
            } else if filter == "NDWI" {
                ndwi(&sat_data)
            } else if filter == "SWIR" {
                swir(&sat_data)
            } else {
                true_color(&sat_data)
            };

            // convert image to jpg
            let mut buffer = Vector::new();
            opencv::imgcodecs::imencode(".jpg", &m, &mut buffer, &Default::default()).unwrap();

            let g_client = self.google_client.clone();
            let id_clone = id.to_string();

            // precache images
            thread::spawn(move || {
                // upload
                Runtime::new().unwrap().block_on(upload_image_to_bucket(&g_client, id_clone.as_str(), "True Color", &sat_data));
                Runtime::new().unwrap().block_on(upload_image_to_bucket(&g_client, id_clone.as_str(), "False Color", &sat_data));
                Runtime::new().unwrap().block_on(upload_image_to_bucket(&g_client, id_clone.as_str(), "NDWI", &sat_data));
                Runtime::new().unwrap().block_on(upload_image_to_bucket(&g_client, id_clone.as_str(), "SWIR", &sat_data));
            });

            // return image
            buffer.to_vec()
        }
    }
}