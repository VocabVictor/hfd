mod types;
mod utils;
mod download;
mod cli;
mod config;

use pyo3::prelude::*;
use crate::download::ModelDownloader;

#[pymodule]
fn hfd(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ModelDownloader>()?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
}

#[pyfunction]
fn main() -> PyResult<()> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let argv: Vec<String> = sys.getattr("argv")?.extract()?;
        
        if argv.len() <= 1 || argv[1] == "-h" || argv[1] == "--help" {
            cli::print_help();
            return Ok(());
        }

        let model_id = &argv[1];
        let mut local_dir = None;
        let mut include_patterns = Vec::new();
        let mut exclude_patterns = Vec::new();
        let mut token = None;
        let mut config_path = None;
        let mut i = 2;
        
        while i < argv.len() {
            match argv[i].as_str() {
                "--config" => {
                    if i + 1 < argv.len() {
                        config_path = Some(argv[i + 1].clone());
                        i += 2;
                    } else {
                        eprintln!("Error: --config requires a file path");
                        return Ok(());
                    }
                }
                "--local-dir" => {
                    if i + 1 < argv.len() {
                        local_dir = Some(argv[i + 1].clone());
                        i += 2;
                    } else {
                        eprintln!("Error: --local-dir requires a directory path");
                        return Ok(());
                    }
                }
                "--hf_token" | "--token" => {
                    if i + 1 < argv.len() {
                        token = Some(argv[i + 1].clone());
                        i += 2;
                    } else {
                        eprintln!("Error: --hf_token requires a token");
                        return Ok(());
                    }
                }
                "--include" => {
                    i += 1;
                    while i < argv.len() && !argv[i].starts_with("--") {
                        include_patterns.push(argv[i].clone());
                        i += 1;
                    }
                }
                "--exclude" => {
                    i += 1;
                    while i < argv.len() && !argv[i].starts_with("--") {
                        exclude_patterns.push(argv[i].clone());
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }

        let config = if let Some(path) = config_path {
            config::Config::load_from_path(&path)
        } else {
            config::Config::load()
        };

        let token = token.or(config.hf_token);

        let downloader = ModelDownloader::new(
            local_dir,
            if include_patterns.is_empty() { None } else { Some(include_patterns) },
            if exclude_patterns.is_empty() { None } else { Some(exclude_patterns) },
            token,
        )?;

        match downloader.download(model_id) {
            Ok(msg) => println!("{}", msg),
            Err(e) => eprintln!("Error: {}", e),
        }

        Ok(())
    })
} 