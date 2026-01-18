.PHONY: help build test check clean bench wasm setup fmt clippy doc verify examples release all ci

help:
	@echo "MielinOS Development Commands"
	@echo "=============================="
	@echo ""
	@echo "  make build    - Build all crates"
	@echo "  make test     - Run all tests"
	@echo "  make check    - Run all quality checks"
	@echo "  make verify   - Verify project health"
	@echo "  make clean    - Clean build artifacts"
	@echo "  make bench    - Run benchmarks"
	@echo "  make wasm     - Build WASM examples"
	@echo "  make examples - Run all examples"
	@echo "  make setup    - Initial project setup"
	@echo "  make fmt      - Format code"
	@echo "  make clippy   - Run linter"
	@echo "  make doc      - Generate documentation"
	@echo ""

build:
	@echo "Building MielinOS..."
	@cargo build --workspace

release:
	@echo "Building release..."
	@cargo build --release --workspace

test:
	@echo "Running tests..."
	@cargo nextest run --workspace

check:
	@echo "Running quality checks..."
	@cargo fmt --all -- --check
	@cargo clippy --all-features --workspace -- -D warnings
	@cargo check --all-features --workspace
	@cargo nextest run --all-features --workspace

clean:
	@echo "Cleaning build artifacts..."
	@cargo clean
	@rm -f Cargo.lock
	@find examples -name "Cargo.lock" -delete 2>/dev/null || true
	@find examples -type d -name "target" -exec rm -rf {} + 2>/dev/null || true

bench:
	@echo "Running benchmarks..."
	@cargo bench -p benches

wasm:
	@echo "Building WASM examples..."
	@rustup target add wasm32-unknown-unknown
	@cargo build -p counter-agent --target wasm32-unknown-unknown --release
	@ls -lh target/wasm32-unknown-unknown/release/*.wasm

setup:
	@echo "Setting up development environment..."
	@./scripts/setup.sh

fmt:
	@echo "Formatting code..."
	@cargo fmt --all

clippy:
	@echo "Running clippy..."
	@cargo clippy --all-features --workspace -- -D warnings

doc:
	@echo "Generating documentation..."
	@cargo doc --all-features --workspace --no-deps --open

examples:
	@echo "Running examples..."
	@cargo run -p hello-agent
	@echo ""
	@cargo run -p agent-migration

install-tools:
	@echo "Installing development tools..."
	@cargo install cargo-nextest --locked || true
	@cargo install cargo-watch || true
	@cargo install cargo-audit || true
	@rustup target add wasm32-unknown-unknown

verify:
	@echo "Verifying project health..."
	@./scripts/verify.sh

all: fmt clippy build test

ci: check
