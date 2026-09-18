# tpack Makefile
#
# Default odin toolchain is resolved from PATH. Override with `make ODIN=/path/to/odin`.

ODIN ?= odin
ODIN_ROOT := $(shell $(ODIN) root)

BIN      := tpack
# All package files; incremental.odin guard so edits trigger a rebuild too.
SRC      := main.odin config.odin atlas.odin sprites.odin packer.odin sprite_map.odin util.odin incremental.odin
# External deps used by the packer: rect_pack plus the whole utils package
# (config/sprite_map/sprites import it via the `dss` vendor collection).
# Wildcard catches any change in it.
UTILS_SRC := $(wildcard $(ODIN_ROOT)/vendor/dss/utils/*.odin)

.PHONY: all build vet test harness clean install

all: build

## build - Compile the tpack binary (default -o:minimal).
build: $(BIN)

$(BIN): $(SRC) $(ODIN_ROOT)/vendor/dss/rect_pack/rect_pack.odin $(UTILS_SRC)
	$(ODIN) build .

## vet - Compile with -vet (unused/shadowing/type checks).
vet:
	$(ODIN) build . -vet

## test - Run `odin test` (no unit tests defined; harness covers integration).
test:
	$(ODIN) test .

## harness - Run the integration harness (packs test/sprites, writes textures).
harness:
	$(ODIN) run test/pack

## clean - Remove the build artifact.
clean:
	rm -f $(BIN)

## install - Install binary to $(PREFIX)/bin (default /usr/local).
PREFIX ?= /usr/local
install: build
	install -m 0755 $(BIN) $(PREFIX)/bin/$(BIN)

## help - Show available targets.
help:
	@echo "Available targets:"
	@grep '^## ' $(MAKEFILE_LIST) | sed 's/^## //'
