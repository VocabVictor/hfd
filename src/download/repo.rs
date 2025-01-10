use super::downloader::ModelDownloader;
use crate::types::RepoInfo;
use pyo3::prelude::*;
use serde_json::Value;

impl ModelDownloader {
    pub(crate) async fn get_repo_info(&self, model_id: &str) -> PyResult<RepoInfo> {
        let url = self.config.get_api_endpoint(model_id);
        println!("Debug: Fetching repo info from URL: {}", url);
        
        // 添加认证头
        let mut request = self.client.get(&url);
        if let Some(token) = &self.auth.token {
            println!("Debug: Using authentication token");
            request = request.header("Authorization", format!("Bearer {}", token));
        } else {
            println!("Debug: No authentication token provided");
        }
        
        let response = request
            .send()
            .await
            .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(format!("获取仓库信息失败: {}", e)))?;

        println!("Debug: Response status: {}", response.status());

        // 先获取响应内容
        let response_text = response.text().await
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("读取响应内容失败: {}", e)))?;
        println!("Debug: Response content: {}", response_text);

        // 尝试解析为 JSON
        let json_value: Value = serde_json::from_str(&response_text)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("解析 JSON 失败: {}", e)))?;

        // 检查是否有错误信息
        if let Some(error) = json_value.get("error") {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "API 返回错误: {}. 请确保提供了正确的 token（可以从 https://huggingface.co/settings/tokens 获取）",
                error
            )));
        }

        // 检查是否有必要的字段
        if !json_value.get("siblings").is_some() && !json_value.get("files").is_some() {
            println!("Debug: Response does not contain siblings or files fields");
            println!("Debug: Available fields: {:?}", json_value.as_object().map(|obj| obj.keys().collect::<Vec<_>>()));
            return Err(pyo3::exceptions::PyValueError::new_err(
                "API 响应格式不正确，缺少必要的文件信息字段"
            ));
        }

        // 尝试解析为 RepoInfo
        let repo_info: RepoInfo = serde_json::from_str(&response_text)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("解析仓库信息失败: {}", e)))?;

        println!("Debug: Successfully parsed repo info");
        println!("Debug: Repo info: {:?}", repo_info);
        Ok(repo_info)
    }
} 