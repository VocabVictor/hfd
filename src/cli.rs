pub fn print_help() {
    println!("Usage:");
    println!("  hfd <REPO_ID> [OPTIONS]");
    println!("\nDescription:");
    println!("  Downloads a model or dataset from Hugging Face using the provided repo ID.");
    println!("\nArguments:");
    println!("  REPO_ID         The Hugging Face repo ID (Required)");
    println!("                  Format: 'org_name/repo_name' or legacy format (e.g., gpt2)");
    println!("\nOptions:");
    println!("  --include       Patterns to include files for downloading (supports multiple patterns).");
    println!("                  e.g., '--include vae/* *.bin'");
    println!("  --exclude       Patterns to exclude files from downloading (supports multiple patterns).");
    println!("                  e.g., '--exclude *.safetensor *.md'");
    println!("  --hf_username   Hugging Face username for authentication (not email).");
    println!("  --hf_token      Hugging Face token for authentication.");
    println!("  --tool          Download tool to use: aria2c (default) or wget.");
    println!("  -x              Number of download threads for aria2c (default: 4).");
    println!("  -j              Number of concurrent downloads for aria2c (default: 5).");
    println!("  --dataset       Flag to indicate downloading a dataset.");
    println!("  --local-dir     Directory path to store the downloaded data.");
    println!("                  Defaults to the current directory with a subdirectory named 'repo_name'");
    println!("  -h, --help      Show this help message");
    println!("\nExamples:");
    println!("  hfd gpt2");
    println!("  hfd bigscience/bloom-560m --exclude *.safetensors");
    println!("  hfd meta-llama/Llama-2-7b --hf_username myuser --hf_token mytoken -x 4");
    println!("  hfd lavita/medical-qa-shared-task-v1-toy --dataset");
} 