use std::path::PathBuf;
use std::env;
use anyhow::{Result, anyhow};
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
struct RepoMetadata {
    siblings: Vec<FileInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileInfo {
    rfilename: String,
}

#[derive(Debug)]
pub struct HFDownloader {
    repo_id: String,
    token: Option<String>,
    local_dir: PathBuf,
    endpoint: String,
}

impl HFDownloader {
    pub fn new(repo_id: &str, token: Option<String>, local_dir: Option<&str>) -> Self {
        let endpoint = env::var("HF_ENDPOINT")
            .unwrap_or_else(|_| "https://huggingface.co".to_string());
        
        let local_dir = local_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(repo_id.split('/').last().unwrap_or(repo_id)));

        Self {
            repo_id: repo_id.to_string(),
            token,
            local_dir,
            endpoint,
        }
    }

    pub async fn download(&self) -> Result<()> {
        // Create client with optional token
        let mut client = Client::builder();
        if let Some(token) = &self.token {
            client = client.default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", token).parse()?,
                );
                headers
            });
        }
        let client = client.build()?;

        // Get repository metadata
        let metadata = self.get_repo_metadata(&client).await?;

        // Create local directory
        fs::create_dir_all(&self.local_dir).await?;

        // Download files
        for file_info in metadata.siblings {
            self.download_file(&client, &file_info).await?;
        }

        Ok(())
    }

    async fn get_repo_metadata(&self, client: &Client) -> Result<RepoMetadata> {
        let url = format!("{}/api/models/{}", self.endpoint, self.repo_id);
        let response = client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Failed to get metadata: {}", response.status()));
        }

        let metadata = response.json().await?;
        Ok(metadata)
    }

    async fn download_file(&self, client: &Client, file_info: &FileInfo) -> Result<()> {
        let file_url = self.get_download_url(&file_info.rfilename);
        let local_path = self.local_dir.join(&file_info.rfilename);

        // Create parent directories if they don't exist
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Start download with progress bar
        let response = client.get(&file_url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading {}", file_info.rfilename));

        let mut file = File::create(&local_path).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }

        pb.finish_with_message(format!("Downloaded {}", file_info.rfilename));
        Ok(())
    }

    fn get_download_url(&self, filename: &str) -> String {
        format!("{}/{}/resolve/main/{}", self.endpoint, self.repo_id, filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download() {
        let downloader = HFDownloader::new("gpt2", None, Some("test-model"));
        downloader.download().await.unwrap();
    }
} 