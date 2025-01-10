use reqwest::Client;
use crate::types::{FileInfo, RepoInfo};
use crate::auth::Auth;
use pyo3::prelude::*;

pub async fn get_repo_info(
    client: &Client,
    endpoint: &str,
    repo_id: &str,
    auth: &Auth,
) -> PyResult<RepoInfo> {
    // 先尝试作为 model 获取
    let model_url = format!("{}/api/models/{}", endpoint, repo_id);
    let mut request = client.get(&model_url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get repo info: {}", e)))?;

    if response.status().is_success() {
        let repo_info: RepoInfo = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        return Ok(repo_info);
    }

    // 如果不是 model，尝试作为 dataset 获取
    let dataset_url = format!("{}/api/datasets/{}", endpoint, repo_id);
    let mut request = client.get(&dataset_url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get repo info: {}", e)))?;

    if response.status().is_success() {
        let repo_info: RepoInfo = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        return Ok(repo_info);
    }

    // 如果都不是，返回错误
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "Repository {} not found or unauthorized. Please check the repository ID and your access token if it's a private repository.",
        repo_id
    )))
} 