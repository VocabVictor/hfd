#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub token: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum RepoFiles {
    Model {
        files: Vec<FileInfo>,
    },
    Dataset {
        siblings: Vec<FileInfo>,
    },
}

#[derive(Debug, serde::Deserialize)]
pub struct RepoInfo {
    #[serde(flatten)]
    pub files: RepoFiles,
    #[serde(default)]
    pub gated: serde_json::Value,
    #[serde(default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct FileInfo {
    pub rfilename: String,
    pub size: Option<u64>,
} 