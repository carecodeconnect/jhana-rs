#!/usr/bin/env bash
# End-to-end NPU sanity check for the Rock 5A.
#
# Walks through every layer that needs to work before the LLM (rkllm) can
# load on the RK3588 NPU:
#   1. Driver:   RKNPU module + debugfs version
#   2. Device:   /sys/class/iommu, /dev/dri presence, IOMMU groups
#   3. Memory:   total, free, CMA, /proc/cmdline cma= value
#   4. Runtime: librkllmrt.so + librknnrt.so on disk
#   5. RKNN:     spawn `test_stt /tmp/quick.wav` if a sample exists
#                (proves the smaller RKNN runtime works end-to-end)
#   6. RKLLM:    spawn `test_rkllm` with a 5-min timeout and capture the
#                exact init log + any dmesg errors that fire during load
#
# Use this after every kernel-related change (driver upgrade, cma= edit,
# overlay swap) to confirm the NPU stack is still healthy.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  set -e
  TS=\$(date +%s)
  echo '=== 1. RKNPU kernel driver ==='
  sudo modinfo rknpu 2>/dev/null | grep -E '^(filename|version|description|author)' | head
  echo
  sudo cat /sys/kernel/debug/rknpu/version 2>&1 | head -1 || echo 'rknpu debugfs not readable'

  echo
  echo '=== 2. NPU + IOMMU ==='
  ls /sys/class/iommu 2>&1 | head
  ls -la /dev/dri 2>&1 | grep -E 'card|renderD' || true
  echo
  ls /sys/devices/platform/fdab0000.npu/ 2>&1 | head

  echo
  echo '=== 3. Memory + cmdline ==='
  grep -E 'MemTotal|MemFree|MemAvailable|^Cma' /proc/meminfo
  echo
  echo 'cmdline:'
  cat /proc/cmdline | tr ' ' '\n' | grep -E 'cma|swiotlb|coherent_pool|hugepages|kasan' || echo '(no relevant flags on cmdline)'

  echo
  echo '=== 4. Runtime libraries ==='
  for lib in /usr/lib/aarch64-linux-gnu/librkllmrt.so /usr/lib/aarch64-linux-gnu/librknnrt.so /usr/lib/librkllmrt.so /usr/lib/librknnrt.so; do
    if [ -e \"\$lib\" ]; then
      ls -la \"\$lib\"
    fi
  done

  echo
  echo '=== 5. RKNN smoke test (small runtime — should always succeed) ==='
  if [ -f /tmp/jhana-e2e-in.wav ] && [ -x ~/jhana-rs/target/debug/test_stt ]; then
    echo 'Running test_stt against the last STT recording...'
    cd ~/jhana-rs && timeout 60 ./target/debug/test_stt /tmp/jhana-e2e-in.wav 2>&1 | grep -E 'Inference|Segments|Text:|Error' | head -8
  else
    echo '(no /tmp/jhana-e2e-in.wav or test_stt — run scripts/test-stt-tts.sh first)'
  fi

  echo
  echo '=== 6. RKLLM (large model) load test — 5-min timeout ==='
  sudo pkill -f test_rkllm 2>/dev/null || true
  sudo pkill -f jhana-rs 2>/dev/null || true
  sleep 1
  sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null
  echo 1 | sudo tee /proc/sys/vm/compact_memory > /dev/null 2>&1 || true
  DMESG_BEFORE=\$(sudo dmesg | wc -l)
  echo 'starting test_rkllm...'
  cd ~/jhana-rs && timeout 300 ./target/debug/test_rkllm 2>&1 | tail -25 || echo '(test_rkllm exited or timed out)'
  echo
  echo '--- new RKNPU dmesg since load started ---'
  sudo dmesg | tail -n +\$DMESG_BEFORE | grep -iE 'rknpu|rkllm|iova|dma|alloc' | tail -10
"
