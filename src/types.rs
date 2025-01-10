use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub token: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RepoInfo {
    pub siblings: Vec<FileInfo>,
    pub files: Vec<FileInfo>,
    pub gated: serde_json::Value,
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct FileInfo {
    pub rfilename: String,
    pub size: Option<u64>,
} 