#!/bin/bash

# 设置版本号
VERSION="v0.0.7"

# 确保工作目录干净
if [[ -n $(git status -s) ]]; then
    echo "Error: Working directory is not clean. Please commit all changes first."
    exit 1
fi

# 更新 Cargo.toml 中的版本号
sed -i "s/^version = \".*\"/version = \"${VERSION#v}\"/" Cargo.toml

# 更新 conda/meta.yaml 中的版本号
sed -i "s/{% set version = \".*\" %}/{% set version = \"${VERSION#v}\" %}/" conda/meta.yaml

# 提交版本更新
git add Cargo.toml conda/meta.yaml
git commit -m "chore: bump version to ${VERSION}"

# 推送更改
git push origin main

# 删除已存在的标签（如果有）
git tag -d $VERSION 2>/dev/null || true
git push origin :refs/tags/$VERSION 2>/dev/null || true

# 创建新标签
git tag $VERSION
git push origin $VERSION

echo "Version ${VERSION} has been tagged and pushed successfully!" 