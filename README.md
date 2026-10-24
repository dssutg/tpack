# tpack

A texture packing CLI written in [Rust](https://www.rust-lang.org). Packs `.png` sprites from a directory into a single texture atlas image and generates a sprite map describing where each sprite landed (C, JSON, SSM, or binary SPR formats).

## Features

- Recursively discovers PNG sprites in a directory (deterministic, sorted order).
- Packs them into a configurable atlas using the skyline algorithm (bottom-left or best-fit heuristics) from the bundled `lib/rect_pack` crate.
- Configurable per-sprite padding to prevent edge bleeding.
- Outputs the atlas as a PNG plus a sprite map in one of several formats (C, JSON, SSM, binary SPR).
- No-overwrite guard and progress reporting to stderr.

## Requirements

- A recent Rust toolchain (tested with Rust 1.99).
- No system libraries: PNG decoding/encoding uses the pure-Rust `image` crate.

## Build and Run

```
cargo build            # produces ./target/debug/tpack
cargo run -- <args>    # build and run in one step
cargo build --release  # produces ./target/release/tpack
```

The binary can also be built and installed with a Makefile (see below).

## Usage

```
tpack [options] <sprites_dir> <atlas_path> <sprite_map_path>
```

Arguments:

| Argument          | Description                                      |
|-------------------|--------------------------------------------------|
| `sprites_dir`     | Directory to scan recursively for `.png` sprites |
| `atlas_path`      | Output atlas image path (PNG)                    |
| `sprite_map_path` | Output sprite map path                           |

Options:

| Flag                    | Default    | Description                                                                       |
|-------------------------|------------|-----------------------------------------------------------------------------------|
| `--width <n>`           | `2048`     | Atlas width (8–4096; ignored with `--auto-size`)                                  |
| `--height <n>`          | `2048`     | Atlas height (8–4096; ignored with `--auto-size`)                                 |
| `--auto-size`           | `on`       | Pick the smallest square power-of-two atlas that fits all sprites (default)       |
| `--no-auto-size`        |            | Use the explicit/default `--width`/`--height` instead of auto-sizing              |
| `--spr-root <dir>`      |            | Project root: `spr` hash paths relative to this directory                         |
| `--sprite-padding <n>`  | `0`        | Padding pixels around each sprite (0–128)                                         |
| `--heuristic <h>`       | `bl`       | Packing heuristic: `bl` (bottom-left, fast) or `bf` (best-fit, tighter)           |
| `--scale-mode <m>`      | `linear`   | SPR atlas scale mode: `nearest`, `linear`, or `pixel-art`                         |
| `--no-sort`             |            | Disable largest-first pre-sorting                                                 |
| `--map-format <f>`      | `auto`     | Sprite map format: `auto`, `c`, `structured-c`, `json`, `ssm`, `spr`              |
| `--map-enum-prefix <p>` | `SPRITE_`  | Enum prefix for `structured-c` output                                             |
| `--map-array-name <n>`  | `sprites`  | Array name for `c` / `structured-c` output                                        |
| `--map-array-type <t>`  | `SDL_Rect` | Element type for `structured-c` output                                            |
| `--no-overwrite`        |            | Refuse to overwrite existing output files                                         |
| `--incremental`         |            | Skip the rebuild when the atlas and sprite map are newer than every source sprite |
| `-h`, `-help`           |            | Show help                                                                         |

The sprite map format is inferred from the output file extension when `--map-format auto` (the default): `.json`, `.ssm`, `.c`, and `.spr` are recognized; `structured-c` requires the explicit flag.

Auto-sizing is the default: unless `--no-auto-size` is given (or `--width`/`--height` are explicitly provided, which disables it), the tool ignores `--width`/`--height` and probes increasing square power-of-two sizes (64 → 128 → … up to 4096) until the skyline packer fits every sprite, then uses the smallest size that works. This is handy when the sprite set varies or you always want the tightest GPU-friendly texture. A sprite wider or taller than 4096 (or a set that outgrows 4096×4096) can never fit and is reported as an `AtlasTooSmall` error.

### Sprite map formats

- **JSON**: an object keyed by sprite image path with `x`, `y`, `w`, `h` rectangles.
- **SSM**: tab-separated records, one per sprite: `S\t<path>\t<x>\t<y>\t<w>\t<h>`.
- **C**: a plain C array of `{ filename, x, y, w, h }` entries.
- **structured-c**: a C header with a generated `enum` of sprite identifiers plus an array indexed by those enum values, guarded by `<prefix>_IMPLEMENTATION` so it can be included for declarations and instantiated in one translation unit.
- **SPR**: a compact binary atlas sprite map (see `docs/spr-format-spec.md`). Each sprite is identified by the FNV-1a 32-bit hash of its source path, so lookups are `O(log n)` binary searches over the hash array. By default the FULL discovered path is hashed (e.g. `assets/units/christina/0001.png`); pass `--spr-root <dir>` to hash paths relative to a project root instead (e.g. with `--spr-root assets`, `assets/units/christina/0001.png` hashes as `units/christina/0001.png`). The atlas scale mode for SPR is emitted as the configured `--scale-mode` (linear by default).

### Incremental builds

With `--incremental`, `tpack` first checks whether a rebuild is actually needed. The atlas PNG and the sprite map file are both required to exist **and** be newer than every source sprite under `sprites_dir` (source directory mtimes are counted too, so adding, removing, or renaming a sprite triggers a rebuild). If the outputs are already current, `tpack` prints that they are up to date and exits `0` without re-reading any sprite or rewriting any file.

Build *configuration* (atlas size, padding, heuristic, map format, ...) is assumed not to change between runs; staleness is detected purely from filesystem modification times of the source files, so only source edits force a repack. Without `--incremental`, `tpack` always does a full rebuild (the default, unchanged behavior).

### Example

```
tpack --width 2048 --height 2048 --map-format json \
      assets/sprites assets/out/atlas.png assets/out/sprites.json
```

## Exit codes

- `0` — success (including `-h`/`-help`)
- `1` — packing or I/O failure (e.g. atlas too small, output exists with `--no-overwrite`), or an image failed to load
- `2` — invalid command-line arguments

Progress and diagnostics are written to stderr so stdout remains clean for scripting.

## Build System

A Makefile provides common targets:

```
make build     # compile ./target/release/tpack
make vet       # run clippy, denying warnings
make harness   # run the integration harness (pack test/sprites)
make clean     # remove build artifacts
make install   # install to $(PREFIX)/bin (default /usr/local)
make help      # list targets
```

Override the toolchain with `make CARGO=/path/to/cargo`.

## Testing

Unit tests cover the rectangle packer (`lib/rect_pack`); run them with `cargo test --workspace`. Integration coverage is provided by `tests/harness.rs`, a harness that spawns the built tool against the fixtures in `test/sprites/`:

```
make harness       # or: cargo test --test harness -- --nocapture
```

This regenerates the harness outputs under `test/textures/`. Reference outputs (e.g. `test/textures/parse_json.c`) illustrate how the JSON/SSM maps are consumed.

## Repository Layout

The tool is a single Rust crate (`tpack`) split across modules by concern, plus the bundled packer crate:

| File                | Responsibility                                         |
|---------------------|--------------------------------------------------------|
| `src/main.rs`       | CLI parsing + entry point                              |
| `src/config.rs`     | constants, enums, option structs                       |
| `src/atlas.rs`      | packing orchestration + atlas PNG output               |
| `src/sprites.rs`    | sprite discovery + decoding                            |
| `src/packer.rs`     | skyline rect packing + pixel compositing               |
| `src/sprite_map.rs` | sprite map output writers (C/JSON/SSM/SPR)             |
| `src/incremental.rs`| mtime-based output staleness check for `--incremental` |
| `src/util.rs`       | shared helpers (escaping, identifier checks)           |
| `lib/rect_pack/`    | standalone skyline rectangle packer crate              |
| `tests/harness.rs`  | process-spawning integration harness                   |

## License

[MIT](LICENSE)
