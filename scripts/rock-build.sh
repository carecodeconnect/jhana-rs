#!/usr/bin/env bash
# Build and test jhana-rs on the Rock 5A.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/rock-ssh.sh" "source ~/.cargo/env && cd ~/jhana-rs && export RUSTFLAGS='-C target-feature=+fp16' && cargo check && cargo build && cargo test"
