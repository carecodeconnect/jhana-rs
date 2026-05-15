#!/usr/bin/env bash
# scripts/rock-kmscon-setup.sh
#
# One-shot setup script: installs kmscon on the Rock, deploys the
# kmscon config + jhana-rs-kmscon systemd unit, and switches the boot
# path from `getty@tty1` → `kmsconvt@tty1` (via the jhana-rs-kmscon
# unit which conflicts with both).
#
# kmscon is a userspace VT daemon that renders the console via
# DRM/KMS with FreeType. Unlike the in-kernel framebuffer console
# (TERM=linux), kmscon can rasterise unicode quadrant block characters
# (▘▝▖▗▀▄▌▐) at sub-cell resolution — which is exactly what
# tui-big-text needs to render scaled letters. See docs/17_DISPLAY.md.
#
# Usage:
#   ./scripts/rock-kmscon-setup.sh                 # run from the dev machine,
#                                                  # talks to the Rock via SSH
#   ssh ubuntu@rock-5a bash -s < scripts/rock-kmscon-setup.sh  # equivalent
#
# Re-running is idempotent. To revert: see "Rollback" at the bottom.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROCK_HOST="${ROCK_HOST:-rock-5a}"
ROCK_USER="${ROCK_USER:-ubuntu}"

step() { echo; echo "── $* ──"; }

step "1/5  Install kmscon on the Rock (apt)"
ssh "${ROCK_USER}@${ROCK_HOST}" "sudo apt-get update -qq && sudo apt-get install -y kmscon fonts-dejavu-core"

step "2/5  Deploy kmscon.conf"
scp "$REPO_ROOT/hardware/kmscon.conf" "${ROCK_USER}@${ROCK_HOST}:/tmp/kmscon.conf"
ssh "${ROCK_USER}@${ROCK_HOST}" "sudo install -D -m 644 /tmp/kmscon.conf /etc/kmscon/kmscon.conf"

step "3/5  Deploy jhana-rs-kmscon.service"
scp "$REPO_ROOT/hardware/jhana-rs-kmscon.service" \
    "${ROCK_USER}@${ROCK_HOST}:/tmp/jhana-rs-kmscon.service"
ssh "${ROCK_USER}@${ROCK_HOST}" "sudo install -D -m 644 /tmp/jhana-rs-kmscon.service /etc/systemd/system/jhana-rs-kmscon.service"

step "4/5  Stop and disable the old getty@tty1 + jhana-rs unit"
ssh "${ROCK_USER}@${ROCK_HOST}" "
    sudo systemctl stop jhana-rs.service 2>/dev/null || true
    sudo systemctl disable jhana-rs.service 2>/dev/null || true
    sudo systemctl stop getty@tty1.service 2>/dev/null || true
"

step "5/5  Enable and start jhana-rs-kmscon"
ssh "${ROCK_USER}@${ROCK_HOST}" "
    sudo systemctl daemon-reload
    sudo systemctl enable --now jhana-rs-kmscon.service
    sleep 4
    sudo systemctl is-active jhana-rs-kmscon.service
"

echo
echo "kmscon is now hosting jhana-rs on tty1."
echo "The Rock's DSI display should show TTF-rasterised glyphs now."
echo
echo "Log:    scripts/rock-log.sh"
echo "Status: ssh ${ROCK_USER}@${ROCK_HOST} systemctl status jhana-rs-kmscon"
echo
echo "─── Rollback ─────────────────────────────────────────"
echo "  ssh ${ROCK_USER}@${ROCK_HOST}"
echo "  sudo systemctl disable --now jhana-rs-kmscon.service"
echo "  sudo systemctl enable --now jhana-rs.service       # back to getty + Linux VT"
echo "──────────────────────────────────────────────────────"
