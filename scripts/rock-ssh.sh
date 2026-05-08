#!/usr/bin/env bash
# SSH into the Rock 5A.
ROCK_IP="${ROCK_IP:-192.168.1.83}"
sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@"$ROCK_IP" "$@"
