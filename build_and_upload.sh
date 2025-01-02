#!/bin/bash

set -e  # Exit on error

# Print commands being executed
set -x

# Activate micromamba environment
eval "$(micromamba shell hook --shell bash)"
micromamba activate base

# Function to increment version
increment_version() {
    local version=$1
    local position=$2
    local new_version=$(echo "$version" | awk -F. -v pos="$position" '{
        if (pos == 1) $1 = $1 + 1;
        else if (pos == 2) $2 = $2 + 1;
        else $3 = $3 + 1;
        print $1"."$2"."$3
    }')
    echo "$new_version"
}

# Get current version from pyproject.toml
CURRENT_VERSION=$(grep '^version = ' hfd-py/pyproject.toml | cut -d'"' -f2)
echo "Current version: ${CURRENT_VERSION}"

# Increment patch version
NEW_VERSION=$(increment_version "$CURRENT_VERSION" 3)
echo "New version: ${NEW_VERSION}"

# Update version in pyproject.toml
sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" hfd-py/pyproject.toml

# Update version in Cargo.toml
sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" hfd-py/Cargo.toml

# Clean previous builds
rm -rf target/wheels/*

# Build the package with ABI3 forward compatibility
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1

# Build in hfd-py directory
echo "Building wheel in hfd-py directory..."
cd hfd-py
maturin build --release
cd ..

# Check if wheel was built successfully
if [ -d "target/wheels" ] && [ "$(ls -A target/wheels)" ]; then
    echo "Found wheel files in target/wheels:"
    ls -l target/wheels
    
    # Check wheel version
    WHEEL_VERSION=$(ls target/wheels/hfd-*.whl | grep -oP 'hfd-\K[0-9]+\.[0-9]+\.[0-9]+(?=-)')
    echo "Wheel version: ${WHEEL_VERSION}"
    
    # Upload to TestPyPI
    echo "Uploading to TestPyPI..."
    twine upload --repository testpypi target/wheels/* --verbose
    
    echo "Upload completed successfully!"
    echo "Package should be available at: https://test.pypi.org/project/hfd/${WHEEL_VERSION}/"

    # Commit version changes
    echo "Committing version changes..."
    git add hfd-py/pyproject.toml hfd-py/Cargo.toml
    git commit -m "Bump version to ${NEW_VERSION}"
else
    echo "Error: No wheel files found in target/wheels/"
    exit 1
fi 