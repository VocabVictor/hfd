use crate::types::FileInfo;
use reqwest::Client;
use serde_json::Value;

pub async fn get_file_list(
    client: &Client, 
    endpoint: &str, 
    model_id: &str, 
    token: Option<String>, 
    is_dataset: bool
) -> Result<Vec<FileInfo>, String> {
    // 构建 API URL
    let api_url = if is_dataset {
        format!("{}/api/datasets/{}", endpoint, model_id)
    } else {
        format!("{}/api/models/{}", endpoint, model_id)
    };

    // 发送请求获取仓库信息
    let mut request = client.get(&api_url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send()
        .await
        .map_err(|e| format!("Failed to get repository info: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to get repository info: {}", response.status()));
    }

    let json: Value = response.json()
        .await
        .map_err(|e| format!("Failed to parse repository info: {}", e))?;

    // 从 siblings 字段获取文件列表
    let siblings = json["siblings"].as_array()
        .ok_or_else(|| "No files found in repository".to_string())?;

    let mut files = Vec::new();
    for file in siblings {
        if let Some(rfilename) = file["rfilename"].as_str() {
            files.push(FileInfo {
                rfilename: rfilename.to_string(),
                size: None,  // 大小会在 repo.rs 中获取
            });
        }
    }

    Ok(files)
} 