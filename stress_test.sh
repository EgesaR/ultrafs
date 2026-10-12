#!/bin/bash

echo "Starting UltraFS Automated Batch Stress Test..."

cargo build --release

# Pipe a sequence of commands directly into the REPL binary
./target/release/ultrafs <<EOF
write note alpha First system entry stream.
write task beta Complete SIMD vector integration.
ingest document1 High-throughput immutable storage architecture with FastCDC.
ingest document2 Secondary payload stream verifying deduplication boundaries.
info
stats
verify
exit
EOF

echo "Batch stress test finished successfully!"