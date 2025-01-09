#!/bin/bash

# 获取当前版本号
CURRENT_VERSION=$(cat Cargo.toml | grep '^version =' | sed 's/version = "\(.*\)"/\1/')
echo "Current version: ${CURRENT_VERSION}"

# 解析版本号
IFS='.' read -r major minor patch <<< "${CURRENT_VERSION}"

# 根据规则递增版本号
# 如果 patch 版本是 10，则将 minor 版本加 1，patch 重置为 0
if [ "$patch" -eq "10" ]; then
    minor=$((minor + 1))
    patch=0
else
    patch=$((patch + 1))
fi

# 组合新版本号
VERSION="v${major}.${minor}.${patch}"
VERSION_WITHOUT_V="${VERSION#v}"
echo "New version will be: ${VERSION}"

# 检查是否有未提交的更改，如果有则提交
if [[ -n $(git status -s) ]]; then
    echo "Found uncommitted changes, committing them first..."
    git add .
    git commit -m "v${VERSION_WITHOUT_V}"
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
git commit -m "Bump version to ${VERSION}"

# 推送更改
git push origin main

# 删除已存在的标签（如果有）
git tag -d $VERSION 2>/dev/null || true
git push origin :refs/tags/$VERSION 2>/dev/null || true

# 创建新标签
git tag $VERSION
git push origin $VERSION

echo "Version ${VERSION} has been tagged and pushed successfully!" 