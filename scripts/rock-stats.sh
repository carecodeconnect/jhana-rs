#!/usr/bin/env bash
# Lightweight nvidia-smi-style snapshot for the Rock 5A: CPU, RAM, NPU
# load + temp, jhana-rs process RSS, latest pipeline timings from the
# TUI log.
#
# Usage:
#   scripts/rock-stats.sh            # one-shot snapshot
#   scripts/rock-stats.sh watch      # loop every 2 s (Ctrl-C to stop)
#   scripts/rock-stats.sh watch 5    # loop every 5 s
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:-once}"
INTERVAL="${2:-2}"

# All probing happens on the Rock in one SSH session per snapshot so we
# don't pay the SSH handshake cost on every loop iteration when watching.
snapshot() {
  "$SCRIPT_DIR/rock-ssh.sh" "bash -s" <<'REMOTE'
    set -u

    # CPU usage: read /proc/stat twice with a small gap, compute %.
    read_cpu() {
      awk '/^cpu / { i=$2+$3+$4+$6+$7+$8; t=$2+$3+$4+$5+$6+$7+$8; print i, t }' /proc/stat
    }
    A=($(read_cpu)); sleep 0.5; B=($(read_cpu))
    DI=$((B[0]-A[0])); DT=$((B[1]-A[1]))
    if [[ $DT -gt 0 ]]; then CPU_PCT=$(awk -v i="$DI" -v t="$DT" 'BEGIN{printf "%5.1f", i*100/t}'); else CPU_PCT="  n/a"; fi

    LOADAVG=$(awk '{print $1, $2, $3}' /proc/loadavg)

    # Memory
    MEM_TOTAL=$(awk '/MemTotal/ {print $2}' /proc/meminfo)
    MEM_FREE=$(awk '/MemAvailable/ {print $2}' /proc/meminfo)
    MEM_USED=$((MEM_TOTAL - MEM_FREE))
    SWAP_TOTAL=$(awk '/SwapTotal/ {print $2}' /proc/meminfo)
    SWAP_FREE=$(awk '/SwapFree/ {print $2}' /proc/meminfo)
    SWAP_USED=$((SWAP_TOTAL - SWAP_FREE))
    CMA=$(awk '/CmaTotal/ {print $2}' /proc/meminfo)

    # NPU: /sys/kernel/debug/rknpu/ requires root.
    NPU=$(sudo cat /sys/kernel/debug/rknpu/load 2>/dev/null | sed -E 's/^NPU load: //; s/ +/ /g' || echo 'n/a')
    NPU_FREQ=$(sudo cat /sys/kernel/debug/rknpu/freq 2>/dev/null | head -1 || echo 'n/a')

    # Temperatures
    declare -A TZ
    for f in /sys/class/thermal/thermal_zone*; do
      [ -f "$f/type" ] || continue
      t=$(cat "$f/type")
      c=$(awk -v v="$(cat $f/temp)" 'BEGIN{printf "%.1f", v/1000}')
      TZ[$t]=$c
    done

    # jhana-rs process info
    PID=$(pgrep -f 'target/.*jhana-rs$' | head -1 || true)
    if [[ -n "$PID" ]]; then
      RSS=$(awk '/VmRSS/ {print $2}' /proc/$PID/status)
      VSZ=$(awk '/VmSize/ {print $2}' /proc/$PID/status)
      THREADS=$(awk '/Threads/ {print $2}' /proc/$PID/status)
      PROC="pid=$PID rss=${RSS} kB vsz=${VSZ} kB threads=${THREADS}"
    else
      PROC='not running'
    fi

    # Latest interesting log lines (stage timings)
    LOG=/home/ubuntu/jhana-rs/jhana-rs.log
    LATEST=""
    if [[ -f $LOG ]]; then
      LATEST=$(grep -E 'STT inference|RKLLM model|TTS:|button:|TTS thread|sentence:|Recorded to|loaded in|preload' "$LOG" 2>/dev/null | tail -6)
    fi

    printf '\n'
    printf '%s\n' '======================================================================'
    printf '  rock-5a stats  %s\n' "$(date +'%Y-%m-%d %H:%M:%S')"
    printf '%s\n' '----------------------------------------------------------------------'
    printf '  CPU       %s%% used    load %s\n' "$CPU_PCT" "$LOADAVG"
    printf '  Memory    %.1f GiB used / %.1f GiB total   (CMA reserved %s kB)\n' \
            "$(awk -v u="$MEM_USED"  'BEGIN{printf "%.1f", u/1024/1024}')" \
            "$(awk -v t="$MEM_TOTAL" 'BEGIN{printf "%.1f", t/1024/1024}')" \
            "$CMA"
    printf '  Swap      %.1f GiB used / %.1f GiB total\n' \
            "$(awk -v u="$SWAP_USED"  'BEGIN{printf "%.1f", u/1024/1024}')" \
            "$(awk -v t="$SWAP_TOTAL" 'BEGIN{printf "%.1f", t/1024/1024}')"
    printf '  NPU       %s\n' "$NPU"
    printf '  NPU freq  %s\n' "$NPU_FREQ"
    printf '  Temps     SoC %s°C  A76 %s°C  A55 %s°C  NPU %s°C  GPU %s°C\n' \
           "${TZ[soc-thermal]:-?}" "${TZ[bigcore0-thermal]:-?}" "${TZ[littlecore-thermal]:-?}" \
           "${TZ[npu-thermal]:-?}" "${TZ[gpu-thermal]:-?}"
    printf '  jhana-rs  %s\n' "$PROC"
    printf '%s\n' '----------------------------------------------------------------------'
    printf '  Recent pipeline log:\n'
    if [[ -n "${LATEST:-}" ]]; then
      while IFS= read -r line; do printf '    %s\n' "$line"; done <<< "$LATEST"
    else
      printf '    (no jhana-rs.log yet)\n'
    fi
    printf '%s\n' '======================================================================'
REMOTE
}

case "$MODE" in
  once)
    snapshot
    ;;
  watch)
    while true; do
      clear
      snapshot
      sleep "$INTERVAL"
    done
    ;;
  *)
    echo "usage: $0 [once|watch [interval_seconds]]" >&2
    exit 2
    ;;
esac
