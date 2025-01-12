use std::fs;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tokio::sync::Semaphore;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use futures::StreamExt;
use reqwest::Client;
use crate::download::DownloadManager;
use crate::INTERRUPT_FLAG;
use crate::types::FileInfo;

pub async fn download_chunked_file(
    file: &FileInfo,
    client: &Client,
    download_manager: Arc<DownloadManager>,
) -> Result<(), String> {
    let _downloaded_size = if let Ok(metadata) = fs::metadata(&file.rfilename) {
        metadata.len()
    } else {
        0
    };

    let chunk_size = download_manager.get_config().chunk_size as u64;
    let total_size = file.size.ok_or_else(|| "File size is required".to_string())?;
    let total_chunks = (total_size + chunk_size - 1) / chunk_size;

    let semaphore = Arc::new(Semaphore::new(download_manager.get_config().connections_per_download));
    let mut tasks = Vec::new();

    for chunk_index in 0..total_chunks {
        let start = chunk_index * chunk_size;
        let end = std::cmp::min(start + chunk_size, total_size);

        let client = client.clone();
        let file_clone = file.clone();
        let semaphore = semaphore.clone();
        let download_manager = download_manager.clone();

        let task = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.map_err(|e| e.to_string())?;

            let response = client
                .get(&format!("{}/resolve/main/{}", download_manager.get_config().endpoint, file_clone.rfilename))
                .header("Range", format!("bytes={}-{}", start, end - 1))
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !response.status().is_success() {
                let error_text = response.text().await.map_err(|e| e.to_string())?;
                return Err(format!("Failed to download chunk: {}", error_text));
            }

            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&file_clone.rfilename)
                .await
                .map_err(|e| e.to_string())?;

            let mut _bytes_written = 0;
            let mut last_update = Instant::now();
            let mut stream = response.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                if INTERRUPT_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
                    return Err("Download interrupted".to_string());
                }

                let chunk = chunk_result.map_err(|e| e.to_string())?;
                let bytes_len = chunk.len() as u64;

                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                _bytes_written += bytes_len;

                let now = Instant::now();
                if now.duration_since(last_update) >= Duration::from_secs(1) {
                    // Update progress
                    last_update = now;
                }
            }

            Ok(())
        });

        tasks.push(task);
    }

    for (chunk_index, task) in tasks.into_iter().enumerate() {
        if let Err(e) = task.await.map_err(|e| e.to_string())? {
            return Err(format!("Chunk {} failed: {}", chunk_index, e));
        }
    }

    Ok(())
} 