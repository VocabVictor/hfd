mod auth;
mod config;
mod download;
mod types;
mod cli;

pub use auth::Auth;
pub use config::Config;
pub use download::ModelDownloader;

pub fn download_model(
    model_id: &str,
    cache_dir: Option<String>,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    token: Option<String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut downloader = ModelDownloader::new(
        cache_dir,
        include_patterns,
        exclude_patterns,
        token,
    )?;
    
    let model_id = model_id.to_string();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        downloader.download_model(&model_id).await.map_err(|e| e.into())
    })
}

pub fn main() {
    if let Some(args) = cli::parse_args() {
        match download_model(
            &args.model_id,
            args.local_dir,
            args.include_patterns,
            args.exclude_patterns,
            args.hf_token,
        ) {
            Ok(result) => println!("{}", result),
            Err(e) => println!("Error: {}", e),
        }
    }
} 