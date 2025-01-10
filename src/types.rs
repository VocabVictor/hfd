use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoInfo {
    #[serde(default)]
    pub siblings: Vec<FileInfo>,
    #[serde(default)]
    pub gated: Value,
    #[serde(default)]
    pub files: Vec<FileInfo>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileInfo {
    #[serde(rename = "rfilename")]
    pub rfilename: String,
    pub size: Option<u64>,
} 