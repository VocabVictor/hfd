use super::downloader::ModelDownloader;
use crate::types::RepoInfo;
use pyo3::prelude::*;

impl ModelDownloader {
    pub(crate) async fn get_repo_info(&self, model_id: &str) -> PyResult<RepoInfo> {
        let url = format!("{}/api/models/{}", self.config.endpoint, model_id);
        
        // 添加认证头
        let mut request = self.client.get(&url);
        if let Some(token) = &self.auth.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request
            .send()
            .await
            .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(format!("获取仓库信息失败: {}", e)))?;

        if response.status().is_redirection() {
            let new_url = response.headers()
                .get("location")
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("无效的重定向 URL")
                })?;
            
            let mut request = self.client.get(new_url);
            if let Some(token) = &self.auth.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }
            
            request
                .send()
                .await
                .map_err(|e| pyo3::exceptions::PyConnectionError::new_err(format!("获取仓库信息失败: {}", e)))?
                .json::<RepoInfo>()
                .await
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("解析仓库信息失败: {}", e)))
        } else {
            response
                .json::<RepoInfo>()
                .await
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("解析仓库信息失败: {}", e)))
        }
    }
} 