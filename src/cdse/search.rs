use std::io::Read;

use crate::cdse::search_result::{parse_search_result, SearchResult};

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