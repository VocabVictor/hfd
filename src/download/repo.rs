use crate::types::RepoInfo;
use pyo3::prelude::*;
use reqwest::Client;
use crate::types::AuthInfo;

pub(crate) async fn get_repo_info(
    client: &Client,
    endpoint: &str,
    model_id: &str,
    auth: &AuthInfo,
) -> PyResult<RepoInfo> {
    // 先尝试作为模型获取
    let model_url = format!("{}/api/models/{}", endpoint, model_id);
    println!("Debug: Trying model endpoint: {}", model_url);
    
    let mut request = client.get(&model_url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }
    
    let response = request.send().await;
    
    match response {
        Ok(resp) if resp.status().is_success() => {
            let repo_info: RepoInfo = resp.json().await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
            Ok(repo_info)
        }
        _ => {
            // 如果模型获取失败，尝试作为数据集获取
            let dataset_url = format!("{}/api/datasets/{}", endpoint, model_id);
            println!("Debug: Trying dataset endpoint: {}", dataset_url);
            
            let mut request = client.get(&dataset_url);
            if let Some(token) = &auth.token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }
            
            let response = request.send().await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to fetch repo info: {}", e)))?;
            
            if !response.status().is_success() {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to fetch repo info from both model and dataset endpoints. Please check:\n1. Repository ID '{}' is correct\n2. You have proper access token for private repos\n3. The repository exists",
                    model_id
                )));
            }
            
            let repo_info: RepoInfo = response.json().await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
            Ok(repo_info)
        }
    }
} 