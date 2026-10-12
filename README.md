# UltraFS

UltraFS is a Rust-based filesystem experiment built around a command-line REPL, content-addressed metadata, block-level deduplication, and integrity inspection. Its design brings together SIMD-assisted processing, FastCDC chunking, BLAKE3 hashing, a compressed LRU cache, a Merkle DAG, filesystem snapshots, and an experimental neural latent reconstruction path using Candle.

At startup, interact with the system through the REPL prompt:

```text
ufs [Global]>
```

## Core capabilities

- **SIMD-assisted processing:** Uses an AVX-256 L1-norm kernel as part of payload analysis.
- **Chunked ingestion:** Uses FastCDC to split incoming content into chunks.
- **Content-based deduplication:** Uses BLAKE3 hashes to identify matching content and avoid storing duplicate blocks where supported by the storage path.
- **Cache and storage pipeline:** Stages compressed chunks in an LRU cache and resolves file reads through the cache/disk path.
- **Merkle DAG metadata:** Tracks filesystem nodes and their relationships using content hashes.
- **Integrity inspection and recovery:** Checks node hashes and physical block references, with a recovery command for attempted self-healing.
- **Snapshots and rollback:** Supports checkpoints and rollback to a previous filesystem state.
- **Access-control metadata:** Stores permission strings on file nodes and supports permission updates.
- **Experimental AI reconstruction:** Dispatches input to a local Candle multi-layer perceptron through `ai-reconstruct`.

## Quick command reference

The following commands are available through the REPL. Exact support can depend on the current build and the command parser.

### Write and read data

#### `write <type> <filename> <content>`

Writes a payload using the selected type, such as `note` or `task`.

```text
write note system_architecture UltraFS is fully operational with AVX-256 and ACM.
```

The write path analyzes payload entropy, runs the SIMD L1-norm check, and commits data through the configured storage path. The implementation describes a 79 ns L1-norm check; measured performance depends on the hardware, build configuration, and workload.

#### `ingest <filename> <content>`

Ingests content through the chunking and deduplication path.

```text
ingest large_doc High-performance immutable block streams with FastCDC chunking and BLAKE3 hashing.
```

The ingestion path chunks the input with FastCDC, hashes chunks with BLAKE3, checks for reusable content, stages chunks in the compressed LRU cache, and updates filesystem metadata.

#### `read <filename>`

Reads a file and outputs its decoded text.

```text
read system_architecture
```

#### `rm <filename>` / `delete <filename>`

Removes a file from the active filesystem state. Depending on shared block references and the configured storage path, removing a node may release references to its physical blocks.

### Inspect and search

#### `help`

Displays the REPL command manual.

#### `info` / `stats`

Displays filesystem metrics or cache statistics, depending on the command available in the current build. Cache statistics include cached chunk counts and memory footprint.

#### `diskinfo <device>`

Inspects a storage device or reports information about the configured storage target.

#### `search <keyword>`

Searches indexed node names for a keyword.

```text
search architecture
```

#### `list`

Lists indexed nodes.

#### `graph` / `dag-dump`

Displays information about the filesystem's node graph and Merkle DAG. The precise command name may vary by REPL version.

#### `compression-stats`

Displays compression and deduplication statistics when this command is enabled in the current build.

#### `verify`

Audits node hashes and checks whether file block references are present in the block-reference table. It reports detected inconsistencies; it does not by itself guarantee that every physical sector is readable.

#### `monitor`

Displays system telemetry, including active-node and subsystem metrics where implemented. Reported or simulated values should not be treated as hardware benchmarks unless they are measured by the active instrumentation.

### Access control

#### `chmod <permissions> <filename>`

Updates the permission string stored in a file node's metadata.

```text
chmod rwxr-xr-x system_architecture
```

The current metadata model attaches `SysAttributes` to file nodes. Folder nodes do not currently expose the same permission attributes in the supplied schema.

### Snapshots and recovery

#### `checkpoint` / `snap`

Creates a checkpoint of the current filesystem state.

#### `rollback` / `undo`

Restores a previously checkpointed state, subject to the current rollback implementation.

#### `recover`

Attempts to restore a consistent state when supported integrity checks detect problems. Recovery is best-effort; run `verify` afterwards to inspect the resulting state.

### AI reconstruction

#### `ai-reconstruct <filename>`

Runs the local Candle neural reconstruction path for the selected file.

```text
ai-reconstruct system_architecture
```

This path is experimental. Successful inference does not, by itself, establish that the output reproduces the original file. Reconstruction quality depends on the model, its inputs, and the implementation of the latent encoding/decoding path.

### Terminal and shutdown

#### `clear`

Clears the terminal display.

#### `exit`

Requests a graceful shutdown, including serialization of filesystem state and storage synchronization where supported by the active backend.

## Suggested validation workflow

Use the following sequence to exercise the main subsystems:

1. **Inspect the system:** Run `help`, `info`, and `diskinfo` to check the command interface and storage configuration.
2. **Write a small payload:** Use `write note` or `write task` to exercise payload processing and storage.
3. **Read it back:** Run `read <filename>` and compare the output with the input.
4. **Test ingestion and deduplication:** Ingest identical content under multiple filenames, then inspect the available DAG and compression statistics.
5. **Audit integrity:** Run `verify` and review any node-hash or block-reference warnings.
6. **Test metadata updates:** Run `chmod <permissions> <filename>`, then verify that the filesystem's hash and reference-management logic remain consistent.
7. **Test snapshots and rollback:** Create a checkpoint, make a change or remove a test node, roll back, and run `verify` again.
8. **Exercise recovery and shutdown:** Run `recover` only against a test state when possible, inspect the result, and exit gracefully.
9. **Test AI reconstruction separately:** Run `ai-reconstruct <filename>` and evaluate the returned output independently from filesystem integrity tests.

## Implementation notes

- The metadata schema defines `File`, `Folder`, `Collection`, `Workbench`, and `Snapshot` node variants.
- File nodes carry attributes such as owner, permission string, entropy score, latent-path flag, and environment variables.
- A node's hash is derived from its serialized representation using BLAKE3. Changing hashed metadata can change the calculated node hash; graph references and indexes must be updated consistently when mutations occur.
- Performance figures, including nanosecond timings, are workload- and hardware-dependent. Treat them as implementation measurements only when reproduced with a documented benchmark.
- Commands listed as aliases or optional inspection utilities may not be enabled in every build.

## Project status

UltraFS is an actively developed systems project. Validate storage, rollback, deduplication, and integrity behavior with disposable test data before relying on it for important files. In particular, confirm that state persistence, block-reference accounting, and hash propagation behave correctly for the configured backend.
