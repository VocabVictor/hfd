import sys
import argparse
from . import download_model as _download_model

def main():
    parser = argparse.ArgumentParser(description='Download models from Hugging Face')
    parser.add_argument('model_id', help='The model ID to download')
    parser.add_argument('--cache-dir', help='Directory to store downloaded files')
    parser.add_argument('--include', nargs='+', help='Patterns to include')
    parser.add_argument('--exclude', nargs='+', help='Patterns to exclude')
    parser.add_argument('--token', help='HuggingFace token')
    
    args = parser.parse_args()
    
    try:
        result = _download_model(
            args.model_id,
            args.cache_dir,
            args.include,
            args.exclude,
            args.token
        )
        print(result)
        return 0
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

if __name__ == '__main__':
    sys.exit(main())
