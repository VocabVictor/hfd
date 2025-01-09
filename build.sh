#!/bin/bash

# 确保脚本在错误时退出
set -e

# 检查是否安装了必要的工具
command -v maturin >/dev/null 2>&1 || { echo "需要安装 maturin，请运行: pip install maturin"; exit 1; }
command -v twine >/dev/null 2>&1 || { echo "需要安装 twine，请运行: pip install twine"; exit 1; }

# 构建当前平台的 wheel
echo "构建 wheel..."
micromamba run -n base maturin build --release --skip-auditwheel

# 上传到 TestPyPI
echo "正在上传到 TestPyPI..."
# micromamba run -n base python -m twine upload --verbose --repository testpypi target/wheels/*

echo "构建和上传完成！"
echo "你可以使用以下命令安装："
echo "pip install --index-url https://test.pypi.org/simple/ hfd==0.0.6" 

# 安装 wheel
micromamba run -n base pip uninstall hfd -y
micromamba run -n base pip install target/wheels/*.whl
