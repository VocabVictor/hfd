use clap::Parser;
use hfdown::HFDownloader;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Repository ID (e.g., 'gpt2', 'meta-llama/Llama-2-7b')
    repo_id: String,

    /// HuggingFace token for authentication
    #[arg(short, long)]
    token: Option<String>,

    /// Local directory to save files
    #[arg(short, long)]
    local_dir: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let downloader = HFDownloader::new(
        &cli.repo_id,
        cli.token,
        cli.local_dir.as_deref(),
    );

    downloader.download().await?;
    Ok(())
} 