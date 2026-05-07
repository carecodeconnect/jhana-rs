#!/usr/bin/env bash
# SSH into the Rock 5A.
sshpass -p 'ubunturock' ssh -o StrictHostKeyChecking=no ubuntu@192.168.1.102 "$@"
