use reqwest::Client;
use crate::types::{FileInfo, RepoInfo};
use crate::auth::Auth;
use crate::config::Config;
use pyo3::prelude::*;
use serde_json::Value;
use futures::future::join_all;
use tokio::sync::Semaphore;
use std::sync::Arc;

pub async fn get_repo_info(
    client: &Client,
    config: &Config,
    repo_id: &str,
    auth: &Auth,
) -> PyResult<RepoInfo> {
    println!("[DEBUG] get_repo_info called with:");
    println!("[DEBUG] repo_id: {}", repo_id);
    println!("[DEBUG] config.endpoint: {}", config.endpoint);

    // 先尝试作为 model 获取
    let model_url = format!("{}/api/models/{}", config.endpoint, repo_id);
    println!("[DEBUG] Constructed model URL: {}", model_url);
    println!("[DEBUG] Trying model URL: {}", model_url);
    let mut request = client.get(&model_url);
    if let Some(token) = &auth.token {
        println!("[DEBUG] Using auth token");
        request = request.header("Authorization", format!("Bearer {}", token));
    } else {
        println!("[DEBUG] No auth token provided");
    }

    let response = request.send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get repo info: {}", e)))?;

    println!("[DEBUG] Model API response status: {}", response.status());

    if response.status().is_success() {
        let json: Value = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        
        println!("[DEBUG] Successfully parsed model API response");
        let files = extract_files(client, &config.endpoint, repo_id, auth, &json, false).await?;
        let model_endpoint = format!("{}/models/{}", config.endpoint, repo_id);
        println!("[DEBUG] Created model endpoint: {}", model_endpoint);
        return Ok(RepoInfo {
            model_endpoint: Some(model_endpoint),
            dataset_endpoint: None,
            files,
        });
    }

    // 如果不是 model，尝试作为 dataset 获取
    let dataset_url = format!("{}/api/datasets/{}", config.endpoint, repo_id);
    println!("[DEBUG] Constructed dataset URL: {}", dataset_url);
    println!("[DEBUG] Trying dataset URL: {}", dataset_url);
    let mut request = client.get(&dataset_url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get repo info: {}", e)))?;

    println!("[DEBUG] Dataset API response status: {}", response.status());

    if response.status().is_success() {
        let json: Value = response.json()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse repo info: {}", e)))?;
        
        println!("[DEBUG] Successfully parsed dataset API response");
        println!("[DEBUG] Dataset API response: {}", json);
        let files = extract_files(client, &config.endpoint, repo_id, auth, &json, true).await?;
        let dataset_endpoint = format!("{}/datasets/{}", config.endpoint, repo_id);
        println!("[DEBUG] Created dataset endpoint: {}", dataset_endpoint);
        return Ok(RepoInfo {
            model_endpoint: None,
            dataset_endpoint: Some(dataset_endpoint),
            files,
        });
    }

    // 如果都不是，返回错误
    println!("[DEBUG] Repository not found or unauthorized");
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "Repository {} not found or unauthorized. Please check the repository ID and your access token if it's a private repository.",
        repo_id
    )))
}

async fn extract_files(
    client: &Client,
    endpoint: &str,
    repo_id: &str,
    auth: &Auth,
    json: &Value,
    is_dataset: bool,
) -> PyResult<Vec<FileInfo>> {
    let siblings = json["siblings"].as_array()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("No files found in repository"))?;
    
    println!("Found {} files in repository", siblings.len());
    
    // 使用信号量限制并发数
    let semaphore = Arc::new(Semaphore::new(10));
    let client = Arc::new(client.clone());
    let auth = Arc::new(auth.clone());

    let mut tasks = Vec::new();
    for file in siblings {
        if let Some(rfilename) = file["rfilename"].as_str() {
            let client = client.clone();
            let auth = auth.clone();
            let semaphore = semaphore.clone();
            let rfilename = rfilename.to_string();
            let endpoint = endpoint.to_string();
            let repo_id = repo_id.to_string();

            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                resolve_file_info(&client, &endpoint, &repo_id, &rfilename, &auth, is_dataset).await
            }));
        }
    }

    let results = join_all(tasks).await;
    let mut files = Vec::new();
    let mut total_size = 0u64;
    for result in results {
        if let Ok(Ok(file_info)) = result {
            if let Some(size) = file_info.size {
                println!("File: {}, Size: {} bytes", file_info.rfilename, size);
                total_size += size;
            } else {
                println!("File: {}, Size: unknown", file_info.rfilename);
            }
            files.push(file_info);
        }
    }
    println!("Total size of all files: {} bytes ({:.2} MB)", total_size, total_size as f64 / 1024.0 / 1024.0);

    Ok(files)
}

async fn resolve_file_info(
    client: &Client,
    endpoint: &str,
    repo_id: &str,
    rfilename: &str,
    auth: &Auth,
    is_dataset: bool,
) -> PyResult<FileInfo> {
    let url = if is_dataset {
        format!("{}/datasets/{}/resolve/main/{}", endpoint, repo_id, rfilename)
    } else {
        format!("{}/models/{}/resolve/main/{}", endpoint, repo_id, rfilename)
    };

    let mut request = client.head(&url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to resolve file: {}", e)))?;

    let size = response.headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if let Some(s) = size {
        println!("Got size for {}: {} bytes", rfilename, s);
    } else {
        println!("Failed to get size for {}", rfilename);
    }

    Ok(FileInfo {
        rfilename: rfilename.to_string(),
        size,
    })
} 