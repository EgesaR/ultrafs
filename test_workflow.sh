#!/usr/bin/env bash
set -e

echo "=== Cleaning up previous test artifacts ==="
rm -f virtual_flash.img ufs_history.txt

echo "=== Starting UltraFS Automated Test Pipeline ==="

# Create a temporary file to capture engine output
LOG_FILE=$(mktemp)

# Attach heredoc to cargo run
cargo run << 'EOF' 2>&1 | tee "$LOG_FILE"
ingest system_architecture The core neural storage engines use MLP projections and 2KB FastCDC chunking to generate 100-byte Latent Quarks.
ingest config_note System configuration parameters and environment variables are indexed via BLAKE3 hashes for zero-copy Merkle DAG branching.
semsearch "system configuration parameters"
semsearch "neural storage engines"
exit
EOF

echo ""
echo "=== Running Automated Assertions ==="

# Preprocess log: strip ANSI color/escape codes and convert carriage returns to newlines
CLEAN_LOG=$(mktemp)
sed 's/\x1B\[[0-9;]*[JKmsu]//g' "$LOG_FILE" | tr '\r' '\n' > "$CLEAN_LOG"

# Helper function to print debug logs on failure
fail_test() {
    echo "❌ Assertion Failed: $1"
    echo "--- Captured Output Log ---"
    cat "$CLEAN_LOG"
    echo "---------------------------"
    rm -f "$LOG_FILE" "$CLEAN_LOG"
    exit 1
}

# 1. Verify 'system_architecture' was processed and completed
if ! grep -q "system_architecture" "$CLEAN_LOG"; then
    fail_test "'system_architecture' was not found in the output log."
fi

if ! grep -q "Ingestion complete" "$CLEAN_LOG"; then
    fail_test "Ingestion completion message was not found."
fi

# 2. Verify semantic search successfully found matches above threshold
if grep -q "No semantic matches found above similarity threshold" "$CLEAN_LOG"; then
    fail_test "Semantic search yielded no matches above threshold."
fi

# 3. Verify similarity score output exists
if ! grep -q "Similarity Score:" "$CLEAN_LOG"; then
    fail_test "Similarity score printouts are missing."
fi

# Cleanup temporary files
rm -f "$LOG_FILE" "$CLEAN_LOG"

echo "=== All Assertions Passed Successfully! ==="