use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Configuration {
    pub listeners: Vec<String>,
    pub file_root: PathBuf,
    pub repositories: HashMap<String, RepositoryConfiguration>,
}

#[derive(Deserialize, Clone)]
pub struct RepositoryConfiguration {
    pub name: String,
}
