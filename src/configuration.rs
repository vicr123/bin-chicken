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
    pub minisign_key: String,
    pub authenticators: Vec<RepositoryAuthenticator>,
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
pub enum RepositoryAuthenticator {
    #[serde(rename = "github_auth_token_repository")]
    GithubAuthTokenRepository { repository: String },

    #[serde(rename = "open_for_write_access")]
    OpenForWriteAccess { this_is_dangerous: String },
}
