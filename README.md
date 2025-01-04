# HFD - Hugging Face Downloader

A fast and efficient tool for downloading models from Hugging Face.

## Features

- Fast parallel downloads
- Support for multiple platforms (Linux, Windows, macOS)
- Easy to use command-line interface
- Progress bar for download tracking
- Local directory support

## Installation

```bash
pip install hfd
```

## Usage

```bash
# Download a model to the default cache directory
hfd bert-base-uncased

# Download a model to a specific directory
hfd bert-base-uncased --local-dir ./bert

# Use a mirror for faster downloads
HF_ENDPOINT=https://hf-mirror.com hfd bert-base-uncased
```

## License

MIT License
