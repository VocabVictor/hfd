import argparse
from hfd import HFDownloader

def main():
    parser = argparse.ArgumentParser(description='Download Hugging Face models')
    parser.add_argument('repo_id', help='The repository ID (e.g. "gpt2")')
    parser.add_argument('--token', help='Hugging Face token for private repos')
    parser.add_argument('--local-dir', help='Local directory to save the model')
    
    args = parser.parse_args()
    
    downloader = HFDownloader(
        args.repo_id,
        token=args.token,
        local_dir=args.local_dir
    )
    
    downloader.download()

if __name__ == '__main__':
    main() 