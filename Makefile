.PHONY: all build release run test unit-test integration-test workflow-test benchmark release-benchmark clean

# Default target builds the project
all: build

# Build debug binary
build:
	cargo build

# Build optimized release binary
release:
	cargo build --release

# Launch the interactive UltraFS shell
run:
	cargo run

# Run the complete test suite (unit tests, integration tests, and workflow script)
test: unit-test integration-test workflow-test

# Run Rust library unit tests
unit-test:
	cargo test --lib

# Run Cargo integration tests located in tests/
integration-test:
	cargo test --test '*'

# Execute the automated bash workflow test script
workflow-test:
	chmod +x test_workflow.sh
	./test_workflow.sh

# Run benchmarks in debug mode
benchmark:
	cargo test --test latent_benchmark -- --nocapture

# Run benchmarks with release optimizations enabled
release-benchmark:
	cargo test --release --test latent_benchmark -- --nocapture

# Remove local database images, history, and cargo build outputs
clean:
	cargo clean
	rm -f virtual_flash.img ufs_history.txt
	@echo "🧹 UltraFS environment cleaned successfully."