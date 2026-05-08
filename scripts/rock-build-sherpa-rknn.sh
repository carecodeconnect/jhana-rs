#!/usr/bin/env bash
# Build sherpa-onnx with RKNN (NPU) support on the Rock 5A.
#
# This replaces the prebuilt CPU-only static libraries that sherpa-onnx-sys
# downloads from GitHub. After this build, set SHERPA_ONNX_LIB_DIR to point
# Cargo at the RKNN-enabled libraries.
#
# Prerequisites: cmake, gcc, g++, git (all present on Rock)
# Produces: /home/ubuntu/sherpa-onnx-rknn/build/install/lib/
#
# Usage (run ON the Rock, not the X61s):
#   bash scripts/rock-build-sherpa-rknn.sh
#
# After building, rebuild jhana-rs with:
#   SHERPA_ONNX_LIB_DIR=/home/ubuntu/sherpa-onnx-rknn/build/install/lib cargo build --release

set -euo pipefail

WORK_DIR="/home/ubuntu/sherpa-onnx-rknn"
RKNN_TOOLKIT_DIR="/home/ubuntu/rknn-toolkit2"
RKNN_RUNTIME_LIB="/usr/lib/librknnrt.so"

echo "=== Step 1: Install librknnrt.so system-wide ==="

# Check if already installed
if [ -f "$RKNN_RUNTIME_LIB" ]; then
    echo "librknnrt.so already in /usr/lib"
    strings "$RKNN_RUNTIME_LIB" | grep "librknnrt version" || true
else
    # Check if it exists in the Python package (from useful_transformers)
    PYTHON_RKNN="/usr/local/lib/python3.10/dist-packages/useful_transformers/librknnrt.so"
    if [ -f "$PYTHON_RKNN" ]; then
        echo "Found librknnrt.so in useful_transformers, checking version..."
        strings "$PYTHON_RKNN" | grep "librknnrt version" || true
        echo "Copying to /usr/lib..."
        sudo cp "$PYTHON_RKNN" /usr/lib/librknnrt.so
        sudo ldconfig
    else
        echo "No librknnrt.so found. Downloading from rknn-toolkit2 v2.2.0..."
        if [ ! -d "$RKNN_TOOLKIT_DIR" ]; then
            git clone --depth 1 --branch v2.2.0 \
                https://github.com/airockchip/rknn-toolkit2.git "$RKNN_TOOLKIT_DIR"
        fi
        sudo cp "$RKNN_TOOLKIT_DIR/rknpu2/runtime/Linux/librknn_api/aarch64/librknnrt.so" \
            /usr/lib/librknnrt.so
        sudo ldconfig
    fi
fi

echo ""
echo "Verifying librknnrt.so..."
strings /usr/lib/librknnrt.so | grep "librknnrt version"
echo ""

echo "=== Step 2: Clone sherpa-onnx ==="

if [ -d "$WORK_DIR" ]; then
    echo "sherpa-onnx already cloned at $WORK_DIR"
    cd "$WORK_DIR"
    git fetch --tags
else
    git clone https://github.com/k2-fsa/sherpa-onnx "$WORK_DIR"
    cd "$WORK_DIR"
fi

# Use the same version as the Rust crate (1.13.0)
git checkout v1.13.0 2>/dev/null || echo "Tag v1.13.0 not found, using latest"

echo ""
echo "=== Step 3: Build with RKNN support (static libraries) ==="

mkdir -p build
cd build

# Clean previous build if present
rm -rf install

cmake \
    -DSHERPA_ONNX_ENABLE_RKNN=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=./install \
    -DSHERPA_ONNX_ENABLE_TTS=ON \
    -DSHERPA_ONNX_ENABLE_BINARY=OFF \
    ..

# Build with 4 cores (use A76 big cores, leave A55 for system)
make -j4
make install

echo ""
echo "=== Step 4: Verify build ==="

echo "Static libraries:"
ls -lh install/lib/*.a 2>/dev/null | head -20

echo ""
echo "Shared libraries (if any):"
ls -lh install/lib/*.so* 2>/dev/null | head -10 || echo "  (none — static build)"

echo ""
echo "=== Done ==="
echo ""
echo "To use with jhana-rs, rebuild with:"
echo "  SHERPA_ONNX_LIB_DIR=$WORK_DIR/build/install/lib cargo build --release"
echo ""
echo "Then set provider in Rust code:"
echo "  provider: Some(\"rknpu\".into())"
