# Routine Dev Commands

Quick reference for daily development on the Rock 5A.

---

## SSH into the Rock

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102
```

Prerequisites (if not already running):
```bash
sudo ip addr add 192.168.1.1/24 dev enp0s25
# In a separate terminal:
sudo dnsmasq --no-daemon --interface=enp0s25 \
  --dhcp-range=192.168.1.100,192.168.1.200,12h --bind-interfaces
```

## Give the Rock internet

```bash
# X61s
sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -o wlan0 -j MASQUERADE
sudo iptables -A FORWARD -i enp0s25 -o wlan0 -j ACCEPT
sudo iptables -A FORWARD -i wlan0 -o enp0s25 -m state --state RELATED,ESTABLISHED -j ACCEPT

# Rock (via SSH)
sudo ip route add default via 192.168.1.1
echo 'nameserver 8.8.8.8' | sudo tee /etc/resolv.conf
```

## Sync code to Rock

```bash
sshpass -p 'ubunturock' rsync -avz --exclude target/ --exclude '.git/' \
  -e "ssh -o StrictHostKeyChecking=no" \
  ~/projects/jhana-rs/ ubuntu@192.168.1.102:~/jhana-rs/
```

## Build and test on Rock

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "source ~/.cargo/env && cd ~/jhana-rs && cargo check && cargo build && cargo test"
```

## Run TUI on Rock display

```bash
# Suppress kernel console messages (safe -- still goes to dmesg/kern.log)
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S dmesg --console-off"

# Clear tty1
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S bash -c 'echo -e \"\033c\" > /dev/tty1'"

# Launch TUI on physical display
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S bash -c 'cd /home/ubuntu/jhana-rs && TERM=linux setsid ./target/debug/jhana-rs </dev/tty1 >/dev/tty1 2>/dev/tty1 &'"
```

## Stop TUI

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S pkill jhana-rs"
```

## Read TUI log

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "cat /home/ubuntu/jhana-rs/jhana-rs.log"
```

## Tail TUI log (live)

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "tail -f /home/ubuntu/jhana-rs/jhana-rs.log"
```

## Start mistral.rs LLM server

The TUI needs mistralrs-server running on the Rock before pressing ENTER.
Start it once — it persists until the Rock reboots or you kill it.

```bash
# SSH into Rock and start the server (takes ~40s to load model)
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "nohup /home/ubuntu/.cargo/bin/mistralrs-server --port 8321 gguf \
    -m /home/ubuntu/models \
    -f Ministral-3B-Instruct-Q4_K_M.gguf \
    > /home/ubuntu/mistralrs.log 2>&1 &"

# Check it's ready (wait for "listening on" message)
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "tail -5 /home/ubuntu/mistralrs.log"
```

Verify it responds:
```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "curl -s http://localhost:8321/v1/models"
```

## Test the full meditation flow

1. Start mistralrs-server (see above)
2. Sync and build:
   ```bash
   scripts/rock-sync.sh
   scripts/rock-build.sh
   ```
3. Launch TUI on Rock display:
   ```bash
   scripts/rock-run.sh
   ```
4. Press ENTER/→ button on Rock to start meditation
5. Watch text stream sentence-by-sentence with [N] pause markers
6. Use UP/DOWN arrows to scroll
7. Press BACK/← to quit
8. View log:
   ```bash
   scripts/rock-log.sh
   ```

## Restore kernel console messages

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "echo 'ubunturock' | sudo -S dmesg --console-on"
```

## Build rustdoc

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 \
  "source ~/.cargo/env && cd ~/jhana-rs && cargo doc --no-deps"
```

## Check disk space

```bash
sshpass -p 'ubunturock' ssh ubuntu@192.168.1.102 "df -h / && free -h"
```
