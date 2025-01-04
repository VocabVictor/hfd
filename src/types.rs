use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoFile {
    pub rfilename: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    pub siblings: Vec<RepoFile>,
} 