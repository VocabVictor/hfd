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