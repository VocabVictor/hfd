#!/bin/bash

# 设置版本号
VERSION="v0.0.7"
VERSION_WITHOUT_V="${VERSION#v}"

# 检查是否有未提交的更改，如果有则提交
if [[ -n $(git status -s) ]]; then
    echo "Found uncommitted changes, committing them first..."
    git add .
    git commit -m "chore: save uncommitted changes before version bump"
fi

# 更新 Cargo.toml 中的版本号
if [ -f "Cargo.toml" ]; then
    sed -i "s/^version = \".*\"/version = \"${VERSION_WITHOUT_V}\"/" Cargo.toml
    git add Cargo.toml
fi

# 更新 pyproject.toml 中的版本号
if [ -f "pyproject.toml" ]; then
    sed -i "s/^version = \".*\"/version = \"${VERSION_WITHOUT_V}\"/" pyproject.toml
    git add pyproject.toml
fi

# 更新 conda/meta.yaml 中的版本号（如果存在）
if [ -f "conda/meta.yaml" ]; then
    sed -i "s/{% set version = \".*\" %}/{% set version = \"${VERSION_WITHOUT_V}\" %}/" conda/meta.yaml
    git add conda/meta.yaml
fi

# 提交版本更新
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