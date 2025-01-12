use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub use_local_dir: bool,
    #[serde(default = "default_model_dir_base")]
    pub local_dir_base: String,
    #[serde(default = "default_dataset_dir_base")]
    pub dataset_dir_base: String,
    #[serde(default = "default_concurrent_downloads")]
    pub concurrent_downloads: usize,
    #[serde(default)]
    pub max_download_speed: Option<u64>,
    #[serde(default = "default_connections_per_download")]
    pub connections_per_download: usize,
    #[serde(default = "default_parallel_download_threshold")]
    pub parallel_download_threshold: u64,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub hf_username: Option<String>,
    #[serde(default)]
    pub hf_token: Option<String>,
}

fn default_endpoint() -> String {
    "https://huggingface.co".to_string()
}

fn default_model_dir_base() -> String {
    "~/.cache/huggingface/models".to_string()
}

fn default_dataset_dir_base() -> String {
    "~/.cache/huggingface/datasets".to_string()
}

fn default_concurrent_downloads() -> usize {
    4
}

fn default_connections_per_download() -> usize {
    8
}

fn default_parallel_download_threshold() -> u64 {
    50 * 1024 * 1024 // 50MB
}

fn default_buffer_size() -> usize {
    8 * 1024 * 1024 // 8MB
}

fn default_chunk_size() -> usize {
    16 * 1024 * 1024 // 16MB
}

fn default_max_retries() -> usize {
    3
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let config_paths = vec![
            dirs::home_dir().map(|p| p.join(".hfdconfig")),
            Some(PathBuf::from("./.hfdconfig")),
        ];
        let config_paths: Vec<_> = config_paths.into_iter().flatten().collect();

        for path in config_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                println!("Loading config from: {}", path.display());
                if let Ok(config) = toml::from_str(&content) {
                    println!("Successfully loaded config from: {}", path.display());
                    return Ok(config);
                }
            }
        }

        println!("No config file found, using default configuration");
        Ok(Self::default())
    }

    #[allow(dead_code)]
    pub fn get_model_dir(&self, model_id: &str) -> String {
        if self.use_local_dir {
            let base = shellexpand::tilde(&self.local_dir_base).into_owned();
            let path = PathBuf::from(base).join(model_id);
            path.to_string_lossy().into_owned()
        } else {
            format!("models/{}", model_id)
        }
    }
} 