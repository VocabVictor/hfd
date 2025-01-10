use crate::types::FileInfo;
use reqwest::Client;
use futures::future::join_all;
use std::collections::HashMap;
use walkdir::WalkDir;
use tokio;

pub async fn get_file_list(client: &Client, endpoint: &str, _model_id: &str, token: Option<String>, is_dataset: bool) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    let mut tasks = Vec::new();
    let mut file_map = HashMap::new();

    // 先收集所有文件信息
    for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path().to_string_lossy().to_string();
            let rfilename = path.trim_start_matches("./").to_string();
            let file_info = FileInfo {
                rfilename: rfilename.clone(),
                size: None,
            };
            file_map.insert(rfilename.clone(), files.len());
            files.push(file_info);

            // 创建获取文件大小的异步任务
            let client = client.clone();
            let endpoint = endpoint.to_string();
            let token = token.clone();
            let rfilename_clone = rfilename.clone();
            let task = tokio::spawn(async move {
                let url = if is_dataset {
                    format!("{}/datasets/{}/resolve/main/{}", endpoint, _model_id, rfilename)
                } else {
                    format!("{}/{}/resolve/main/{}", endpoint, _model_id, rfilename)
                };

                let mut request = client.head(&url);
                if let Some(token) = token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }
                
                match request.send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            if let Some(size) = response.headers().get("content-length") {
                                if let Ok(size) = size.to_str().unwrap_or("0").parse::<u64>() {
                                    println!("Got size for {}: {} bytes", rfilename_clone, size);
                                    return Some((rfilename_clone, size));
                                }
                            }
                        }
                        println!("Failed to get size for {}: {}", rfilename_clone, response.status());
                        None
                    }
                    Err(e) => {
                        println!("Error getting size for {}: {}", rfilename_clone, e);
                        None
                    }
                }
            });
            tasks.push(task);
        }
    }

    println!("Waiting for {} file sizes...", tasks.len());
    
    // 等待所有任务完成
    let results = join_all(tasks).await;
    
    // 更新文件大小
    let mut total_size = 0u64;
    for result in results {
        if let Ok(Some((rfilename, size))) = result {
            if let Some(&index) = file_map.get(&rfilename) {
                files[index].size = Some(size);
                total_size += size;
            }
        }
    }

    println!("Total size of all files: {} bytes ({:.2} MB)", total_size, total_size as f64 / 1024.0 / 1024.0);
    Ok(files)
} 