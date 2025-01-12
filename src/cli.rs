use std::env;
use pyo3::prelude::*;
use crate::download::repo;
use tokio::runtime::Runtime;
use crate::auth::Auth;
use glob;

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
    let config = crate::config::Config::load()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
    let client = reqwest::Client::new();
    let base_path = if let Some(dir) = local_dir {
        std::path::PathBuf::from(dir)
    } else {
        let base = shellexpand::tilde(&config.local_dir_base).into_owned();
        std::path::PathBuf::from(base)
    };

    // 创建 Auth 对象
    let auth = crate::auth::Auth {
        token: token.clone(),
        username: None,
    };

    // 获取仓库信息
    let repo_info = repo::get_repo_info(
        &client,
        &config,
        &model_id,
        &auth,
    ).await?;

    // 根据仓库信息判断是否为数据集
    let is_dataset = repo_info.is_dataset();

    // 创建下载目录
    let target_path = base_path.join(&model_id);
    tokio::fs::create_dir_all(&target_path)
        .await
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create directory: {}", e)))?;

    // 使用 repo_info 中的文件列表
    let mut files = repo_info.files;

    // 应用文件过滤
    if let Some(patterns) = include_patterns {
        files.retain(|file| {
            patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&file.rfilename))
                    .unwrap_or(false)
            })
        });
    }

    if let Some(patterns) = exclude_patterns {
        files.retain(|file| {
            !patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&file.rfilename))
                    .unwrap_or(false)
            })
        });
    }

    // 下载文件
    crate::download::download_task::download_folder(
        client,
        config.endpoint,
        model_id,
        target_path.clone(),
        target_path.file_name().unwrap().to_string_lossy().to_string(),
        files,
        token,
        is_dataset,
    ).await?;

    Ok(target_path.to_string_lossy().to_string())
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