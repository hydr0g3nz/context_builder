BIN     := gocx
RELEASE := target/release/$(BIN)
DEBUG   := target/debug/$(BIN)

.PHONY: all build release run test lint fmt check clean install help

all: build

build:
	cargo build

release:
	cargo build --release

run:
	cargo run --

run-release:
	cargo run --release --

test:
	cargo test

test-verbose:
	cargo test -- --nocapture

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

check:
	cargo check

clean:
	cargo clean

install: release
	cargo install --path .

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@echo "  build        Build debug binary"
	@echo "  release      Build optimized release binary"
	@echo "  run          Run debug binary (pass args with: make run -- <args>)"
	@echo "  run-release  Run release binary"
	@echo "  test         Run tests"
	@echo "  test-verbose Run tests with output"
	@echo "  lint         Run clippy lints"
	@echo "  fmt          Format source code"
	@echo "  fmt-check    Check formatting without writing"
	@echo "  check        Check compilation without building"
	@echo "  clean        Remove build artifacts"
	@echo "  install      Install binary to cargo bin path"
