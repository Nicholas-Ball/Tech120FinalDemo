#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub file_size: usize,
}

impl SearchResult {
    pub fn new(json: &serde_json::Value) -> SearchResult {
        SearchResult {
            id: json["Id"].as_str().unwrap().to_string(),
            file_size: json["ContentLength"].as_u64().unwrap() as usize,
        }
    }
}

/// Pass the json array here and this will parse it into SearchResult structs
pub fn parse_search_result(json: serde_json::Value) -> Vec<SearchResult> {
    let mut to_return = Vec::new();

    for x in json.as_array().unwrap() {
        to_return.push(
            SearchResult::new(x)
        );
    }

    to_return
}