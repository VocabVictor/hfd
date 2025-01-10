use reqwest::Client;
use crate::types::{FileInfo, RepoInfo};
use crate::auth::Auth;
use pyo3::prelude::*;

pub async fn get_repo_info(
    client: &Client,
    endpoint: &str,
    model_id: &str,
    auth: &Auth,
) -> PyResult<RepoInfo> {
    // 检查是否是数据集
    let is_dataset = model_id.contains("/datasets/") || model_id.contains("datasets/");
    let clean_id = if is_dataset {
        model_id.replace("/datasets/", "/").replace("datasets/", "")
    } else {
        model_id.to_string()
    };

    // 构建API URL
    let api_url = if is_dataset {
        format!("{}/api/datasets/{}", endpoint, clean_id)
    } else {
        format!("{}/api/models/{}", endpoint, clean_id)
    };

    // 发送请求
    let mut request = client.get(&api_url);
    if let Some(token) = &auth.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get repo info: {}", e)))?
        .error_for_status()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("HTTP error: {}", e)))?;

    let json = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse response: {}", e)))?;

    // 解析文件列表
    let files = if let Some(siblings) = json["siblings"].as_array() {
        siblings.iter()
            .filter_map(|file| {
                file["rfilename"].as_str().map(|name| FileInfo {
                    rfilename: name.to_string(),
                    size: None,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // 构建仓库信息
    let repo_info = RepoInfo {
        model_endpoint: if is_dataset {
            None
        } else {
            Some(format!("{}/{}", endpoint, model_id))
        },
        dataset_endpoint: if is_dataset {
            Some(format!("{}/datasets/{}", endpoint, clean_id))
        } else {
            None
        },
        files,
    };

    Ok(repo_info)
} 