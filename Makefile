# tpack Makefile
#
# Default Rust toolchain is resolved from PATH. Override with `make CARGO=/path/to/cargo`.

CARGO ?= cargo
BIN   := target/release/tpack

.PHONY: all build vet test harness clean install

all: build

## build - Compile the tpack binary (release).
build:
	$(CARGO) build --release

## vet - Lint with clippy, denying warnings.
vet:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## test - Run the unit and integration test suites.
test:
	$(CARGO) test --workspace

## harness - Run the integration harness (packs test/sprites, writes textures).
harness:
	$(CARGO) test --test harness -- --nocapture

## clean - Remove build artifacts.
clean:
	$(CARGO) clean

## install - Install binary to $(PREFIX)/bin (default /usr/local).
PREFIX ?= /usr/local
install: build
	install -m 0755 $(BIN) $(PREFIX)/bin/tpack

## help - Show available targets.
help:
	@echo "Available targets:"
	@grep '^## ' $(MAKEFILE_LIST) | sed 's/^## //'
