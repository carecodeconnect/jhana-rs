#!/usr/bin/env bash
# Benchmark LLM inference on the Rock 5A.
#
# Runs a short prompt through the GGUF model and measures:
# - Time to first token
# - Tokens per second
# - Peak memory usage
#
# Usage: scripts/rock-bench-llm.sh [model_path]
# Default model: /home/ubuntu/models/test-model.gguf
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MODEL="${1:-/home/ubuntu/models/test-model.gguf}"

echo "=== LLM Benchmark ==="
echo "Model: $MODEL"
echo ""

"$SCRIPT_DIR/rock-ssh.sh" "
  source ~/.cargo/env
  cd ~/jhana-rs

  # Check model exists
  if [ ! -f '$MODEL' ]; then
    echo 'ERROR: Model not found: $MODEL'
    echo 'Download a GGUF model first. Example:'
    echo '  curl -L -o /home/ubuntu/models/test-model.gguf \\'
    echo '    https://huggingface.co/Aryanne/Orca-Mini-3B-gguf/resolve/main/q4_0-orca-mini-3b.gguf'
    exit 1
  fi

  # Run benchmark binary (must be built first)
  if [ ! -f target/debug/bench_llm ]; then
    echo 'ERROR: bench_llm binary not found. Run: cargo build --bin bench_llm'
    exit 1
  fi

  # Memory before
  FREE_BEFORE=\$(free -m | awk '/^Mem:/ {print \$4}')
  echo \"Free RAM before: \${FREE_BEFORE} MB\"

  # Run benchmark
  time target/debug/bench_llm '$MODEL'

  # Memory after
  FREE_AFTER=\$(free -m | awk '/^Mem:/ {print \$4}')
  echo \"Free RAM after: \${FREE_AFTER} MB\"
  echo \"RAM used by model: \$((\$FREE_BEFORE - \$FREE_AFTER)) MB\"
"
