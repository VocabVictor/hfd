use super::downloader::ModelDownloader;
use crate::types::RepoInfo;
use pyo3::prelude::*;
use serde_json::Value;

impl ModelDownloader {
    async fn try_get_repo_info(&self, url: &str) -> Option<RepoInfo> {
        println!("Debug: Fetching repo info from URL: {}", url);
        
        // 添加认证头
        let mut request = self.client.get(url);
        if let Some(token) = &self.auth.token {
            println!("Debug: Using authentication token");
            request = request.header("Authorization", format!("Bearer {}", token));
        } else {
            println!("Debug: No authentication token provided");
        }
        
        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                println!("Debug: Request failed: {}", e);
                return None;
            }
        };

        println!("Debug: Response status: {}", response.status());

        if !response.status().is_success() {
            println!("Debug: Request failed with status: {}", response.status());
            return None;
        }

        // 获取响应内容
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                println!("Debug: Failed to read response: {}", e);
                return None;
            }
        };
        println!("Debug: Response content: {}", response_text);

        // 尝试解析为 JSON
        let json_value: Value = match serde_json::from_str(&response_text) {
            Ok(val) => val,
            Err(e) => {
                println!("Debug: Failed to parse JSON: {}", e);
                return None;
            }
        };

        // 检查是否有错误信息
        if json_value.get("error").is_some() {
            println!("Debug: Response contains error");
            return None;
        }

        // 检查是否有必要的字段
        if !json_value.get("siblings").is_some() && !json_value.get("files").is_some() {
            println!("Debug: Response does not contain siblings or files fields");
            println!("Debug: Available fields: {:?}", json_value.as_object().map(|obj| obj.keys().collect::<Vec<_>>()));
            return None;
        }

        // 尝试解析为 RepoInfo
        match serde_json::from_str(&response_text) {
            Ok(repo_info) => {
                println!("Debug: Successfully parsed repo info");
                Some(repo_info)
            }
            Err(e) => {
                println!("Debug: Failed to parse repo info: {}", e);
                None
            }
        }
    }

    pub(crate) async fn get_repo_info(&self, model_id: &str) -> PyResult<RepoInfo> {
        // 先尝试作为模型获取
        let model_url = self.config.get_api_endpoint(model_id);
        if let Some(repo_info) = self.try_get_repo_info(&model_url).await {
            return Ok(repo_info);
        }

        // 再尝试作为数据集获取
        let dataset_url = model_url.replace("/api/models/", "/api/datasets/");
        if let Some(repo_info) = self.try_get_repo_info(&dataset_url).await {
            return Ok(repo_info);
        }

        // 如果都失败了，返回错误
        Err(pyo3::exceptions::PyValueError::new_err(format!(
            "无法获取仓库信息。请检查：\n\
             1. 仓库ID '{}' 是否正确\n\
             2. 如果是私有仓库，请提供正确的 token（可以从 https://huggingface.co/settings/tokens 获取）\n\
             3. 确保仓库类型是模型或数据集",
            model_id
        )))
    }
} 