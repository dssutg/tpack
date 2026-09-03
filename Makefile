# tpack Makefile
#
# Default odin toolchain is resolved from PATH. Override with `make ODIN=/path/to/odin`.

ODIN ?= odin

BIN      := tpack
SRC      := main.odin config.odin atlas.odin sprites.odin packer.odin sprite_map.odin util.odin

.PHONY: all build vet test harness clean install

all: build

## build - Compile the tpack binary (default -o:minimal).
build: $(BIN)

$(BIN): $(SRC) ../dssodinlib/rect_pack/rect_pack.odin
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
