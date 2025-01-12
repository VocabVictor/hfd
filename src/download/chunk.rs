use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use std::io::SeekFrom;
use futures::StreamExt;
use std::time::Duration;
use crate::INTERRUPT_FLAG;
use crate::types::FileInfo;
use super::DownloadManager;

pub async fn download_chunked_file(
    file: &FileInfo,
    client: &Client,
    download_manager: Arc<DownloadManager>,
) -> Result<(), String> {
    let url = format!("{}/resolve/main/{}", download_manager.get_config().endpoint, file.rfilename);
    let progress = download_manager.create_progress(file.rfilename.clone(), file.size);

    let downloaded_size = if let Ok(metadata) = fs::metadata(&file.rfilename) {
        metadata.len()
    } else {
        0
    };

    let chunk_size = download_manager.get_config().chunk_size as u64;
    let total_chunks = (file.size + chunk_size - 1) / chunk_size;
    let start_chunk = downloaded_size / chunk_size;
    let chunks: Vec<_> = (start_chunk..total_chunks).collect();

    let semaphore = Arc::new(Semaphore::new(download_manager.get_config().connections_per_download));
    let (tx, mut rx) = tokio::sync::mpsc::channel(chunks.len());

    let mut tasks = Vec::new();
    for chunk_index in chunks {
        let start = chunk_index * chunk_size;
        let end = std::cmp::min(start + chunk_size, file.size);

        let client = client.clone();
        let url = url.clone();
        let progress = progress.clone();
        let semaphore = semaphore.clone();
        let tx = tx.clone();

        let task = tokio::spawn(async move {
            let max_retries = 3;
            let mut retries = 0;

            loop {
                let _permit = match semaphore.try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };

                let result = async {
                    let response = client
                        .get(&url)
                        .header("Range", format!("bytes={}-{}", start, end - 1))
                        .send()
                        .await?;

                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response.text().await?;
                        return Err(format!("HTTP {} - {}", status, error_text));
                    }

                    let mut stream = response.bytes_stream();
                    let mut file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(&file.rfilename)?;

                    file.seek(SeekFrom::Start(start))?;

                    let mut downloaded = 0;
                    let mut last_update = Instant::now();
                    let mut bytes_since_last_update = 0;

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        file.write_all(&chunk)?;
                        downloaded += chunk.len() as u64;
                        bytes_since_last_update += chunk.len() as u64;
                        progress.inc(chunk.len() as u64);

                        let now = Instant::now();
                        if now.duration_since(last_update).as_secs() >= 1 {
                            let elapsed = now.duration_since(last_update).as_secs_f64();
                            let speed = bytes_since_last_update as f64 / elapsed;
                            bytes_since_last_update = 0;
                            last_update = now;
                        }
                    }

                    Ok(())
                }
                .await;

                match result {
                    Ok(_) => {
                        let _ = tx.send(chunk_index).await;
                        break;
                    }
                    Err(e) => {
                        if retries >= max_retries {
                            return Err(format!("Failed to download chunk after {} retries: {}", max_retries, e));
                        }
                        retries += 1;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            Ok(())
        });
        tasks.push(task);
    }

    for i in 0..tasks.len() {
        if let Some(chunk_index) = rx.recv().await {
            if let Err(e) = tasks[chunk_index as usize].await? {
                return Err(e);
            }
        }
    }

    progress.finish();
    Ok(())
} 