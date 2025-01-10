use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use indicatif::ProgressBar;
use futures::StreamExt;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[allow(dead_code)]
pub async fn download_file_with_chunks(
    client: &Client,
    url: String,
    file_path: PathBuf,
    total_size: u64,
    chunk_size: usize,
    max_retries: usize,
    token: Option<String>,
    progress_bar: Arc<ProgressBar>,
    running: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut file = File::create(&file_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded = 0u64;
    let mut current_retry = 0;

    while downloaded < total_size {
        if !running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let start = downloaded;
        let end = std::cmp::min(downloaded + chunk_size as u64, total_size);
        let mut request = client.get(&url);

        if let Some(token) = &token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request = request.header("Range", format!("bytes={}-{}", start, end - 1));

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                if current_retry < max_retries {
                    current_retry += 1;
                    continue;
                }
                return Err(format!("Failed to download chunk: {}", e));
            }
        };

        if !response.status().is_success() {
            if current_retry < max_retries {
                current_retry += 1;
                continue;
            }
            return Err(format!("HTTP error: {}", response.status()));
        }

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if !running.load(Ordering::SeqCst) {
                return Ok(());
            }

            let chunk = chunk.map_err(|e| format!("Failed to read chunk: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Failed to write chunk: {}", e))?;

            downloaded += chunk.len() as u64;
            progress_bar.inc(chunk.len() as u64);
        }

        current_retry = 0;
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {}", e))?;

    Ok(())
} 