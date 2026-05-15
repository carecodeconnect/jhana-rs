#!/usr/bin/env bash
# Diagnose RKLLM model loading on the Rock 5A.
#
# Background: rkllm_init() can fail with "RKNPU: failed to allocate
# IOVA: -12" + "rknpu_gem_get_pages: dma map fail" when the kernel
# can't satisfy a multi-GB contiguous IOVA mapping. The fix is usually
# either bumping the CMA region or running this test right after boot
# before memory fragments.
#
# This script:
#   1. Kills any running jhana-rs / test_rkllm so the NPU is free.
#   2. Drops page cache and triggers memory compaction.
#   3. Prints CMA, memory, IOMMU, NPU state.
#   4. Runs target/debug/test_rkllm and tails the result.
#   5. Tails the latest RKNPU kernel messages.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/rock-ssh.sh" "
  set -e
  echo '=== 1) free the NPU ==='
  sudo pkill -f jhana-rs 2>/dev/null || true
  sudo pkill -f test_rkllm 2>/dev/null || true
  sleep 1

  echo
  echo '=== 2) drop caches + compact memory ==='
  sync
  echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null
  echo 1 | sudo tee /proc/sys/vm/compact_memory > /dev/null 2>&1 || true

  echo
  echo '=== 3) state ==='
  echo '--- /proc/meminfo (memory + CMA) ---'
  grep -E 'MemTotal|MemFree|MemAvailable|Cma' /proc/meminfo
  echo
  echo '--- kernel cmdline (look for cma=) ---'
  cat /proc/cmdline | tr ' ' '\n' | grep -E 'cma|swiotlb|coherent' || echo '(no cma/swiotlb on cmdline)'
  echo
  echo '--- RKNPU driver ---'
  sudo modinfo rknpu 2>/dev/null | grep -E '^(name|filename|version)' | head -3
  sudo cat /sys/kernel/debug/rknpu/version 2>&1 | head -1
  echo
  echo '--- IOMMU state for fdab0000.npu ---'
  ls /sys/class/iommu/ 2>&1
  cat /sys/kernel/debug/dri/1/name 2>/dev/null | head || true

  echo
  echo '=== 4) run test_rkllm (timeout 120s) ==='
  cd ~/jhana-rs
  timeout 120 ./target/debug/test_rkllm 2>&1 | tail -40 || true

  echo
  echo '=== 5) latest RKNPU kernel messages ==='
  sudo dmesg | grep -iE 'rknpu|rkllm|iova|dma.map' | tail -10
"
