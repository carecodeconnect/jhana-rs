#!/bin/bash
# Remove R installation and related files
set -e
echo "Removing R packages and libraries..."
apt remove -y r-base r-base-core r-base-dev r-base-html r-recommended 2>/dev/null || true
apt autoremove -y 2>/dev/null || true
rm -rf /home/solaris/R
rm -rf /home/solaris/node_modules
echo "Done. Freed space:"
df -h /
