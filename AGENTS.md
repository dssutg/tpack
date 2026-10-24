# tpack

Rust texture packing CLI. Packs `.png` sprites into a texture atlas and generates sprite map output (C, JSON, SSM, SPR).

## Build and Run

```
cargo build              # debug build at target/debug/tpack
cargo run -- <args>      # build + run (note the `--` separator so args go to the tool, not cargo)
cargo build --release    # optimized build at target/release/tpack
make build / vet / harness / clean   # Makefile targets available
```

## Package Layout

- `src/main.rs` — CLI parsing + `main()` entry
- `src/config.rs` — constants, `Map_Format` enum, lookup helpers
- `src/atlas.rs` — `pack_sprites_to_atlas` orchestrator, `Error` enum, atlas PNG output. `pack_sprites_to_atlas` probes increasing power-of-two sizes (from `Min_Auto_Atlas_Size`) when auto-size is in effect (the default; enabled unless `--no-auto-size` or explicit `--width`/`--height`).
- `src/sprites.rs` — sprite discovery/loading, `Sprite`/`Surface` structs, recursive walk; loads sprites in parallel via `src/parallel.rs`
- `src/packer.rs` — skyline rect packing wrapper + `copy_sprites_to_atlas` parallel row-band blit (bounds-clipped)
- `src/sprite_map.rs` — sprite map writers (C/JSON/SSM/SPR), `image_path_to_enum_name`; `write_spr_sprite_map` emits the binary SPR format (little-endian hashes + crops) and `spr_rel_path` strips `--spr-root` from paths before hashing
- `src/incremental.rs` — `--incremental` staleness check: `outputs_are_current` stats both outputs and compares against `newest_sprite_mtime` (recursive, directories included).
- `src/parallel.rs` — order-preserving scoped-thread helpers used for sprite decoding and atlas compositing
- `src/util.rs` — helpers: `escape_json_string`, `escape_ssm_field`, `is_valid_c_identifier`
- `lib/rect_pack/` — the skyline rectangle packer, a standalone Rust crate (port of `stb_rect_pack.h`). `src/lib.rs` holds the algorithm, `tests/rect_pack_test.rs` the tests.

## Gotchas

- **Rect packer dependency:** `lib/rect_pack` is a local path dependency (`rect_pack = { path = "lib/rect_pack" }`). Keep its API and algorithm in sync with `src/packer.rs`.
- **Surface layout:** `Surface` pixels are tightly packed RGBA8 (`pitch` = width × 4), matching the image crate's `RgbaImage`. The atlas blit clips to bounds and forces transparent black where source alpha is 0.
- `fs::create_dir_all` succeeds when the directory already exists — no special "Exist" handling is needed.
- `-h`/`-help` are the only help flags; there is no `--help` flag.
- Sprite map format `auto` is inferred from the output file extension (`.json`/`.ssm`/`.c`/`.spr`); `structured-c` must be requested explicitly.
- Generated files (C/JSON/SSM) must escape filenames via the helpers in `src/util.rs`.
- Case-insensitive extension matching (`.PNG`) is handled by `string_has_suffix_fold` in `src/sprites.rs`.
- **`--auto-size` probes** call `pack_sprites_skyline` repeatedly; each probe overwrites the sprite offsets so the final probe leaves the correct layout.
- **Threading:** `--threads <n>` defaults to `available_parallelism()` and must be at least 1. The skyline packer is sequential; threads parallelize PNG decoding and atlas compositing. Output must stay byte-identical for every thread count (harness checks this).
