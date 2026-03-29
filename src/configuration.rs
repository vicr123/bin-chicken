use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct Configuration {
    pub listeners: Vec<String>,
    pub file_root: String,
    pub repositories: HashMap<String, RepositoryConfiguration>,
}

#[derive(Deserialize, Clone)]
pub struct RepositoryConfiguration {
    pub name: String,
}
