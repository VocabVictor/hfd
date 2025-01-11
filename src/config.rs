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
                if let Ok(value) = toml::from_str::<toml::Value>(&content) {
                    if let Ok(config) = Self::from_toml(value) {
                        return Ok(config);
                    }
                }
            }
        }

        Ok(Self::default())
    }

    pub fn from_toml(value: toml::Value) -> Result<Self, String> {
        let mut config = Config::default();

        println!("Parsing config from TOML:");
        println!("Raw TOML value: {:?}", value);

        if let Some(endpoint) = value.get("endpoint").and_then(|v| v.as_str()) {
            println!("Found endpoint: {}", endpoint);
            config.endpoint = endpoint.to_string();
        }
        if let Some(use_local_dir) = value.get("use_local_dir").and_then(|v| v.as_bool()) {
            println!("Found use_local_dir: {}", use_local_dir);
            config.use_local_dir = use_local_dir;
        }
        if let Some(local_dir_base) = value.get("local_dir_base").and_then(|v| v.as_str()) {
            println!("Found local_dir_base: {}", local_dir_base);
            config.local_dir_base = local_dir_base.to_string();
        }
        if let Some(dataset_dir_base) = value.get("dataset_dir_base").and_then(|v| v.as_str()) {
            println!("Found dataset_dir_base: {}", dataset_dir_base);
            config.dataset_dir_base = dataset_dir_base.to_string();
        }
        if let Some(concurrent_downloads) = value.get("concurrent_downloads").and_then(|v| v.as_integer()) {
            println!("Found concurrent_downloads: {}", concurrent_downloads);
            config.concurrent_downloads = concurrent_downloads as usize;
        }
        if let Some(max_download_speed) = value.get("max_download_speed").and_then(|v| v.as_integer()) {
            println!("Found max_download_speed: {}", max_download_speed);
            config.max_download_speed = Some(max_download_speed as u64);
        }
        if let Some(connections_per_download) = value.get("connections_per_download").and_then(|v| v.as_integer()) {
            println!("Found connections_per_download: {}", connections_per_download);
            config.connections_per_download = connections_per_download as usize;
        }
        if let Some(parallel_download_threshold) = value.get("parallel_download_threshold").and_then(|v| v.as_integer()) {
            println!("Found parallel_download_threshold: {}", parallel_download_threshold);
            config.parallel_download_threshold = parallel_download_threshold as u64;
        }
        if let Some(buffer_size) = value.get("buffer_size").and_then(|v| v.as_integer()) {
            println!("Found buffer_size: {}", buffer_size);
            config.buffer_size = buffer_size as usize;
        }
        if let Some(chunk_size) = value.get("chunk_size").and_then(|v| v.as_integer()) {
            println!("Found chunk_size: {}", chunk_size);
            let chunk_size = chunk_size as usize;
            config.chunk_size = if chunk_size == 0 {
                default_chunk_size()
            } else {
                chunk_size
            };
        }
        if let Some(max_retries) = value.get("max_retries").and_then(|v| v.as_integer()) {
            println!("Found max_retries: {}", max_retries);
            config.max_retries = max_retries as usize;
        }
        if let Some(hf_username) = value.get("hf_username").and_then(|v| v.as_str()) {
            println!("Found hf_username: {}", hf_username);
            config.hf_username = Some(hf_username.to_string());
        }
        if let Some(hf_token) = value.get("hf_token").and_then(|v| v.as_str()) {
            println!("Found hf_token: {}", hf_token);
            config.hf_token = Some(hf_token.to_string());
        }

        println!("Final config: {:?}", config);
        Ok(config)
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