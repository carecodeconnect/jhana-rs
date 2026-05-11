#!/usr/bin/env bash
# Configure Armbian microSD for headless first boot (no keyboard/screen needed).
# Mounts the card, writes autoconfig to skip the first-boot wizard, unmounts.
# Credentials are read from config.json to stay consistent with rock-*.sh scripts.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$SCRIPT_DIR/../config.json"
ROCK_USER="$(jq -r '.rock.user' "$CONFIG")"
ROCK_PASS="$(jq -r '.rock.password' "$CONFIG")"

DEVICE="${1:-/dev/mmcblk0p1}"
MOUNTPOINT="/mnt"

echo "=== Armbian headless first-boot setup ==="
echo "Device:   $DEVICE"
echo "User:     $ROCK_USER"
echo "Password: $ROCK_PASS"
echo ""

# Mount
if mountpoint -q "$MOUNTPOINT"; then
    echo "$MOUNTPOINT already mounted, using existing mount"
else
    echo "Mounting $DEVICE to $MOUNTPOINT..."
    sudo mount "$DEVICE" "$MOUNTPOINT"
fi

# Verify this is an Armbian root filesystem
if [ ! -d "$MOUNTPOINT/root" ]; then
    echo "ERROR: $MOUNTPOINT/root not found — is this an Armbian partition?"
    exit 1
fi

# Write autoconfig
echo "Writing autoconfig to /root/.not_logged_in_yet..."
sudo bash -c "cat > $MOUNTPOINT/root/.not_logged_in_yet << 'EOF'
PRESET_LOCALE=\"en_US.UTF-8\"
PRESET_TIMEZONE=\"Etc/UTC\"
PRESET_ROOT_PASSWORD=\"$ROCK_PASS\"
PRESET_USER_NAME=\"$ROCK_USER\"
PRESET_USER_PASSWORD=\"$ROCK_PASS\"
EOF"

echo "Verifying..."
sudo cat "$MOUNTPOINT/root/.not_logged_in_yet"

# Unmount
echo ""
echo "Unmounting $MOUNTPOINT..."
sudo umount "$MOUNTPOINT"

echo ""
echo "Done. Move the microSD to the Rock and power on."
echo "SSH should be available after boot: sshpass -p '$ROCK_PASS' ssh root@<IP>"
