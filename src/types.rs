use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoInfo {
    pub siblings: Vec<FileInfo>,
    pub gated: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileInfo {
    #[serde(rename = "rfilename")]
    pub rfilename: String,
    pub size: Option<u64>,
} 