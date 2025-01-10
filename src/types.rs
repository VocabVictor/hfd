use serde::Deserialize;
use std::collections::HashMap;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RepoInfo {
    pub siblings: Vec<FileInfo>,
    #[serde(default)]
    pub extra: HashMap<String, Value>,
    #[serde(default)]
    pub cardData: HashMap<String, Value>,
    #[serde(default)]
    pub pipeline_tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileInfo {
    pub rfilename: String,
    #[serde(skip)]
    pub size: Option<u64>,
}

impl RepoInfo {
    pub fn is_dataset(&self) -> bool {
        // 检查 cardData 中的 task_categories 字段
        if let Some(Value::Array(_)) = self.cardData.get("task_categories") {
            return true;
        }
        // 检查是否有 pipeline_tag，如果有说明是模型
        if self.pipeline_tag.is_some() {
            return false;
        }
        // 检查文件列表中是否包含典型的数据集文件
        self.siblings.iter().any(|f| f.rfilename.starts_with("data/") || f.rfilename.contains("dataset"))
    }
} 