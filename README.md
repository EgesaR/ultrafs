# ultrafs

# Reference
Quick Command Reference for the REPL
Once the interface (ufs [Global]> ) appears, try out these core commands to exercise the system:

write <filename> <content>

Example: write note system_architecture UltraFS is fully operational with AVX-256 and ACM.

Action: Analyzes file entropy, executes the 79ns SIMD L1-norm check, and commits it directly to physical sectors via O_DIRECT.

ingest <filename> <content>

Example: ingest large_doc High-performance immutable block streams with FastCDC chunking and BLAKE3 hashing.

Action: Chunkifies the stream, deduplicates identical blocks, stages them in the compressed LRU cache, and updates the Merkle DAG.

read <filename>

Example: read system_architecture

Action: Resolves the file path, pulls blocks through the cache/disk pipeline, and outputs the decoded text.

verify

Action: Audits the entire in-memory Merkle DAG, cross-referencing node hashes and checking block reference counters.

recover

Action: Triggers intelligent self-healing if any sector pointer discrepancies or corrupted nodes are identified.

stats

Action: Inspects current cache pressure, chunk counts, and memory footprints.

exit

Action: Safely serializes the entire filesystem state into the superblock, flushes the hardware cache, and shuts down gracefully.