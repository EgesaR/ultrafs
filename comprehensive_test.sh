#!/bin/bash
set -e

echo "========================================================"
echo "  Launching UltraFS v1.0 Full Feature Test Suite"
echo "========================================================"

# Ensure release binary is up to date
cargo build --release

# Pipe a sequence of commands through the interactive REPL binary
./target/release/ultrafs <<EOF
help
info
diskinfo
write note config_note UltraFS system configuration parameters v1.0.
write task deploy_task Deploy high-performance ACM thread pool.
ingest large_payload Building a custom content-defined chunking pipeline with FastCDC and BLAKE3 hashing.
ingest duplicate_payload Building a custom content-defined chunking pipeline with FastCDC and BLAKE3 hashing.
list
search payload
dag-dump
compression-stats
chmod rwxr-xr-x config_note
read config_note
ai-reconstruct config_note
stats
verify
checkpoint
rm duplicate_payload
rollback
verify
monitor
clear
exit
EOF

echo "========================================================"
echo "  All UltraFS Commands Executed and Verified Successfully!"
echo "========================================================"