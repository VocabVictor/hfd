use std::env;
use pyo3::prelude::*;
use crate::download::download_task::download_folder;
use crate::config::Config;
use crate::types::FileInfo;
use crate::download::repo::get_repo_files;
use clap::{Parser, Subcommand};
use pyo3::prelude::*;
use reqwest::Client;
use std::path::PathBuf;
use crate::download::downloader::ModelDownloader;
use crate::download::repo;
use tokio::runtime::Runtime;

pub struct CliArgs {
    pub model_id: String,
    pub config_path: Option<String>,
    pub include_patterns: Option<Vec<String>>,
    pub exclude_patterns: Option<Vec<String>>,
    pub local_dir: Option<String>,
    pub hf_token: Option<String>,
}

pub fn parse_args() -> Option<CliArgs> {
    let args: Vec<String> = env::args().skip(2).collect();
    
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print_help();
        return None;
    }

    let mut cli_args = CliArgs {
        model_id: args[0].clone(),
        config_path: None,
        include_patterns: None,
        exclude_patterns: None,
        local_dir: None,
        hf_token: None,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                if i + 1 < args.len() {
                    cli_args.config_path = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--include" => {
                let mut patterns = Vec::new();
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    patterns.push(args[i].clone());
                    i += 1;
                }
                if !patterns.is_empty() {
                    cli_args.include_patterns = Some(patterns);
                }
                continue;
            }
            "--exclude" => {
                let mut patterns = Vec::new();
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    patterns.push(args[i].clone());
                    i += 1;
                }
                if !patterns.is_empty() {
                    cli_args.exclude_patterns = Some(patterns);
                }
                continue;
            }
            "--local-dir" => {
                if i + 1 < args.len() {
                    cli_args.local_dir = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--hf_token" => {
                if i + 1 < args.len() {
                    cli_args.hf_token = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    Some(cli_args)
}

pub fn print_help() {
    println!(r#"Usage:
    hfd <REPO_ID> [--config path] [--include pattern1 pattern2 ...] [--exclude pattern1 pattern2 ...] [--local-dir path] [--hf_token token]

Description:
    Downloads a model from Hugging Face using the provided repo ID.

Arguments:
    REPO_ID         The Hugging Face repo ID (Required)
                    Format: 'org_name/repo_name' or legacy format (e.g., gpt2)

Options:
    --config        (Optional) Path to config file
                    Defaults to ~/.hfdconfig or ./.hfdconfig
    --include       (Optional) Patterns to include files for downloading (supports multiple patterns)
    --exclude       (Optional) Patterns to exclude files from downloading (supports multiple patterns)
    --local-dir     (Optional) Directory path to store the downloaded data
    --hf_token      (Optional) Hugging Face token for authentication
                    Can also be configured in config file

Example:
    hfd gpt2
    hfd bigscience/bloom-560m --exclude *.safetensors
    hfd meta-llama/Llama-2-7b --config /path/to/config.toml
    hfd meta-llama/Llama-2-7b --hf_username myuser --hf_token mytoken"#);
}

pub async fn download_file(
    model_id: String,
    local_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
) -> PyResult<String> {
    let downloader = ModelDownloader::new(
        local_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;

    // 获取仓库信息
    let repo_info = repo::get_repo_info(
        &downloader.client,
        &downloader.config,
        &model_id,
        &downloader.auth,
    ).await?;

    // 根据仓库信息判断是否为数据集
    let is_dataset = repo_info.is_dataset();

    // 创建下载目录
    let base_path = std::path::PathBuf::from(&downloader.cache_dir).join(&model_id);
    
    // 下载文件，传入已获取的仓库信息
    downloader.download_files(&model_id, &base_path, is_dataset, repo_info).await?;

    Ok(base_path.to_string_lossy().to_string())
}

pub fn run_cli() -> PyResult<()> {
    if let Some(args) = parse_args() {
        let rt = Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;
            
        match rt.block_on(download_file(
            args.model_id,
            args.local_dir,
            args.include_patterns,
            args.exclude_patterns,
            args.hf_token,
        )) {
            Ok(result) => println!("{}", result),
            Err(e) => println!("Error: {}", e),
        }
    }
    Ok(())
} 