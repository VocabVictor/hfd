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

    pub(crate) async fn get_repo_info(&mut self, model_id: &str) -> PyResult<RepoInfo> {
        // 尝试作为模型仓库获取
        let model_url = format!("{}/api/models/{}", self.config.endpoint, model_id);
        println!("Debug: Fetching repo info from URL: {}", model_url);
        
        let mut request = self.client.get(&model_url);
        if let Some(token) = &self.auth.token {
            request = request.header("Authorization", format!("Bearer {}", token));
            println!("Debug: Using authentication token");
        } else {
            println!("Debug: No authentication token provided");
        }
        
        let response = request.send().await.map_err(|e| {
            pyo3::exceptions::PyConnectionError::new_err(format!("请求失败: {}", e))
        })?;

        println!("Debug: Response status: {}", response.status());
        
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            // 如果未授权，可能需要 token
            println!("Debug: Unauthorized access, token might be required");
        }

        if !response.status().is_success() {
            println!("Debug: Request failed with status: {}", response.status());
            
            // 如果作为模型仓库访问失败，尝试作为数据集仓库
            let dataset_url = format!("{}/api/datasets/{}", self.config.endpoint, model_id);
            println!("Debug: Fetching repo info from URL: {}", dataset_url);
            
            let mut request = self.client.get(&dataset_url);
            if let Some(token) = &self.auth.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }
            
            let response = request.send().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("请求失败: {}", e))
            })?;

            println!("Debug: Response status: {}", response.status());
            
            if !response.status().is_success() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    format!("无法获取仓库信息。请检查：\n1. 仓库ID '{}' 是否正确\n2. 如果是私有仓库，请提供正确的 token（可以从 https://huggingface.co/settings/tokens 获取）\n3. 确保仓库类型是模型或数据集", model_id)
                ));
            }

            let content = response.text().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("读取响应失败: {}", e))
            })?;
            
            println!("Debug: Response content: {}", content);

            let repo_info: RepoInfo = serde_json::from_str(&content).map_err(|e| {
                println!("Debug: Failed to parse repo info: {}", e);
                pyo3::exceptions::PyValueError::new_err(format!(
                    "无法获取仓库信息。请检查：\n1. 仓库ID '{}' 是否正确\n2. 如果是私有仓库，请提供正确的 token（可以从 https://huggingface.co/settings/tokens 获取）\n3. 确保仓库类型是模型或数据集",
                    model_id
                ))
            })?;

            self.repo_info = Some(repo_info.clone());
            Ok(repo_info)
        } else {
            let content = response.text().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("读取响应失败: {}", e))
            })?;
            
            println!("Debug: Response content: {}", content);

            let repo_info: RepoInfo = serde_json::from_str(&content).map_err(|e| {
                println!("Debug: Failed to parse repo info: {}", e);
                pyo3::exceptions::PyValueError::new_err(format!(
                    "无法获取仓库信息。请检查：\n1. 仓库ID '{}' 是否正确\n2. 如果是私有仓库，请提供正确的 token（可以从 https://huggingface.co/settings/tokens 获取）\n3. 确保仓库类型是模型或数据集",
                    model_id
                ))
            })?;

            self.repo_info = Some(repo_info.clone());
            Ok(repo_info)
        }
    }
} 