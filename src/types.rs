use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub rfilename: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub model_endpoint: Option<String>,
    pub dataset_endpoint: Option<String>,
    pub files: Vec<FileInfo>,
}

impl RepoInfo {
    pub fn is_dataset(&self) -> bool {
        self.dataset_endpoint.is_some()
    }
} 