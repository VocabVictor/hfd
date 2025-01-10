use reqwest::Client;
use crate::types::{FileInfo, RepoInfo};
use crate::auth::Auth;
use pyo3::prelude::*;
use serde_json::Value;

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
        let json: Value = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        
        let files = extract_files(&json)?;
        return Ok(RepoInfo {
            model_endpoint: Some(format!("{}/models/{}", endpoint, repo_id)),
            dataset_endpoint: None,
            files,
        });
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
        let json: Value = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        
        let files = extract_files(&json)?;
        return Ok(RepoInfo {
            model_endpoint: None,
            dataset_endpoint: Some(format!("{}/datasets/{}", endpoint, repo_id)),
            files,
        });
    }

    // 如果都不是，返回错误
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "Repository {} not found or unauthorized. Please check the repository ID and your access token if it's a private repository.",
        repo_id
    )))
}

fn extract_files(json: &Value) -> PyResult<Vec<FileInfo>> {
    let siblings = json["siblings"].as_array()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("No files found in repository"))?;
    
    let files = siblings.iter()
        .filter_map(|file| {
            let rfilename = file["rfilename"].as_str()?;
            let size = file["size"].as_u64();
            Some(FileInfo {
                rfilename: rfilename.to_string(),
                size,
            })
        })
        .collect();
    
    Ok(files)
} 