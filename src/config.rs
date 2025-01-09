use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use dirs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    // 镜像地址
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    // 是否使用本地目录
    #[serde(default = "default_use_local_dir")]
    pub use_local_dir: bool,
    // 本地目录基地址
    #[serde(default = "default_local_dir_base")]
    pub local_dir_base: String,
    // 并发下载任务数量
    #[serde(default = "default_concurrent_downloads")]
    pub concurrent_downloads: usize,
    // 单个任务最大下载速度 (bytes/s), 0 表示不限制
    #[serde(default)]
    pub max_download_speed: u64,
    // 单个下载任务最大并发连接数
    #[serde(default = "default_connections_per_download")]
    pub connections_per_download: usize,
    // 达到多大使用并发下载 (bytes)
    #[serde(default = "default_parallel_download_threshold")]
    pub parallel_download_threshold: u64,
    // 下载缓冲区大小 (bytes)
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_model_dir")]
    pub model_dir: String,
    #[serde(default)]
    pub hf_token: Option<String>,
}

fn default_endpoint() -> String {
    "https://huggingface.co".to_string()
}

fn default_use_local_dir() -> bool {
    false
}

fn default_local_dir_base() -> String {
    "~/.cache/huggingface".to_string()
}

fn default_model_dir() -> String {
    "~/.cache/huggingface".to_string()
}

fn default_concurrent_downloads() -> usize {
    3
}

fn default_connections_per_download() -> usize {
    3
}

fn default_parallel_download_threshold() -> u64 {
    100 * 1024 * 1024 // 100MB
}

fn default_buffer_size() -> usize {
    1024 * 1024 // 1MB
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: "https://huggingface.co".to_string(),
            concurrent_downloads: 3,
            max_download_speed: 0,  // 不限速
            connections_per_download: 10,  // 修改默认并发数为10
            parallel_download_threshold: 100 * 1024 * 1024,  // 100MB
            buffer_size: 1024 * 1024,  // 1MB
            use_local_dir: false,
            local_dir_base: "~/.code/models".to_string(),
            model_dir: "~/.cache/huggingface".to_string(),
            hf_token: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_from_default_paths()
    }

    pub fn load_from_path(config_path: &str) -> Self {
        if let Ok(content) = fs::read_to_string(config_path) {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
            eprintln!("警告: 无法解析配置文件 {}", config_path);
        } else {
            eprintln!("警告: 无法读取配置文件 {}", config_path);
        }
        Self::default()
    }

    fn load_from_default_paths() -> Self {
        let config_paths = vec![
            dirs::home_dir().map(|p| p.join(".hfdconfig")),
            Some(PathBuf::from(".hfdconfig")),
        ];

        for config_path in config_paths.into_iter().flatten() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = toml::from_str::<Config>(&content) {
                    return config;
                }
            }
        }

        Self::default()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> std::io::Result<()> {
        let config_path = if let Some(home) = dirs::home_dir() {
            home.join(".hfdconfig")
        } else {
            PathBuf::from(".hfdconfig")
        };

        let contents = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        fs::write(config_path, contents)
    }

    pub fn get_model_dir(&self, model_id: &str) -> String {
        if self.use_local_dir {
            // 获取模型名称（取最后一个部分）
            let model_name = model_id.split('/').last().unwrap_or(model_id);
            // 展开路径中的 ~ 符号
            let base_dir = shellexpand::tilde(&self.local_dir_base).to_string();
            format!("{}/{}", base_dir, model_name)
        } else {
            format!("{}", shellexpand::tilde(&self.local_dir_base))
        }
    }
} 