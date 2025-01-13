#!/bin/bash

# 确保脚本在错误时退出
set -e

# 检查是否安装了必要的工具
command -v maturin >/dev/null 2>&1 || { echo "需要安装 maturin，请运行: pip install maturin"; exit 1; }
command -v twine >/dev/null 2>&1 || { echo "需要安装 twine，请运行: pip install twine"; exit 1; }

# 获取当前版本号
CURRENT_VERSION=$(cat Cargo.toml | grep '^version =' | sed 's/version = "\(.*\)"/\1/')

# 获取最新构建的 wheel 文件
LATEST_WHEEL=$(ls -t target/wheels/*.whl | head -n1)

# 安装 wheel
micromamba run -n base pip uninstall hfd -y
micromamba run -n base pip install "$LATEST_WHEEL"
# micromamba run -n base hfd -h
# micromamba run -n base hfd Wild-Heart/Disney-VideoGeneration-Dataset

# git 提交
if [[ -n $(git status -s) ]]; then
    git add .
    git commit -m "update something [skip actions]"
    git push --force
else
    echo "没有需要提交的更改"
fi