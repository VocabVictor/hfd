use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            use_local_dir: false,
            local_dir_base: default_model_dir_base(),
            dataset_dir_base: default_dataset_dir_base(),
            concurrent_downloads: default_concurrent_downloads(),
            max_download_speed: None,
            connections_per_download: default_connections_per_download(),
            parallel_download_threshold: default_parallel_download_threshold(),
            buffer_size: default_buffer_size(),
            chunk_size: default_chunk_size(),
            max_retries: default_max_retries(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            hf_username: None,
            hf_token: None,
        }
    }
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
    3
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

        let mut config = Self::default();
        println!("Default config: {:#?}", config);

        for path in config_paths {
            println!("Checking config file: {}", path.display());
            if let Ok(content) = fs::read_to_string(&path) {
                println!("Loading config from: {}", path.display());
                println!("Config content:\n{}", content);
                match toml::from_str::<Config>(&content) {
                    Ok(new_config) => {
                        println!("Successfully loaded config from {}: {:#?}", path.display(), new_config);
                        // 合并配置
                        if new_config.concurrent_downloads > 0 {
                            config.concurrent_downloads = new_config.concurrent_downloads;
                        }
                        if new_config.connections_per_download > 0 {
                            config.connections_per_download = new_config.connections_per_download;
                        }
                        config.endpoint = new_config.endpoint;
                        config.use_local_dir = new_config.use_local_dir;
                        config.local_dir_base = new_config.local_dir_base;
                        config.dataset_dir_base = new_config.dataset_dir_base;
                        config.max_download_speed = new_config.max_download_speed;
                        config.parallel_download_threshold = new_config.parallel_download_threshold;
                        config.buffer_size = new_config.buffer_size;
                        config.chunk_size = new_config.chunk_size;
                        config.max_retries = new_config.max_retries;
                        config.include_patterns = new_config.include_patterns;
                        config.exclude_patterns = new_config.exclude_patterns;
                        config.hf_username = new_config.hf_username;
                        config.hf_token = new_config.hf_token;
                    }
                    Err(e) => {
                        println!("Failed to parse config file {}: {}", path.display(), e);
                    }
                }
            }
        }

        println!("Final config: {:#?}", config);
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