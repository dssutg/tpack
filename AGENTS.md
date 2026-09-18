# tpack

Odin texture packing CLI. Packs `.png` sprites into a texture atlas and generates sprite map output (C, JSON, SSM, SPR).

## Build and Run

```
odin build .              # produces ./tpack
odin run . -- <args>      # build + run (note the `--` separator so args go to the tool, not odin)
make build / vet / harness / clean   # Makefile targets available
```

## External Dependency

Imports `vendor:dss/rect_pack` (Odin `vendor:` collection). Must exist at `<odin root>/vendor/dss/rect_pack` (e.g. a `dssodinlib` checkout symlinked at `<odin root>/vendor/dss`) or the build will fail. That package is an external skyline rect packer; do not inline or vendor it here.

## Test Harness

`test/pack/pack.odin` (run via `odin run test/pack` or `make harness`) is a process-spawning harness, NOT an Odin test suite. `odin test .` reports "No tests to run". It packs `test/sprites/` and writes `test/textures/sprites.{png,c}`. The command inside uses the `--` separator to pass args through odin to the binary — keep that if you edit it.

## Package Layout

All files are a single Odin package (`main`) split by concern. Edits must keep cross-file visibility in mind (everything is shared within a package).
- `main.odin` — CLI parsing (core:flags) + `main()` entry
- `config.odin` — constants, `Map_Format` enum, lookup tables. Note: needs `#+feature dynamic-literals` at top (map literals)
- `atlas.odin` — `pack_sprites_to_atlas` orchestrator, `Error` union, atlas PNG output. `pack_sprites_to_atlas` probes increasing power-of-two sizes (from `Min_Auto_Atlas_Size`) when auto-size is in effect (the default; enabled unless `--no-auto-size` or explicit `--width`/`--height`).
- `sprites.odin` — sprite discovery/loading, `Sprite` struct, `free_sprite_surfaces`
- `packer.odin` — skyline rect packing wrapper + `copy_sprite_to_atlas` pixel blit (bounds-clipped)
- `sprite_map.odin` — sprite map writers (C/JSON/SSM/SPR), `image_path_to_enum_name`; `write_spr_sprite_map` emits the binary SPR format (little-endian hashes + crops) and `spr_rel_path` strips `--spr-root` from paths before hashing
- `incremental.odin` — `--incremental` staleness check: `outputs_are_current` stats both outputs and compares against `newest_sprite_mtime` (recursive, directories included). These run before the pack arena exists, so all their allocations use `context.temp_allocator` (never the arena, and they must not `delete`).
- `util.odin` — helpers: `escape_json_string`, `escape_ssm_field`, `is_valid_c_identifier`

## Gotchas

- **Arena memory model:** `main` creates one `virtual.Arena` per pack cycle and passes its allocator down through every allocating function (`find_and_read_sprites`, `find_image_files`, `walk_image_dir`, `read_sprite_from_file`, `pack_sprites_skyline`, `pack_sprites_to_atlas`, `save_texture_atlas`, `save_atlas_sprite_map`). All Odin-side memory (sprite slice, image_path strings, request lists) and error strings (`fmt.aprintf(..., allocator = allocator)`) come from it; `virtual.arena_free_all` reclaims everything on the success path.
- **Never `delete` (or otherwise `.Free`) arena-allocated memory.** The virtual arena's allocator returns `.Mode_Not_Implemented` for `.Free`, which aborts the program. Do not call `delete` on arena slices/dynamic arrays; rely on `free_all`. The only per-allocation frees that remain are: SDL surfaces (via `free_sprite_surfaces`, since arenas don't track C-library allocations) and rect_pack's own internal scratch, which uses the default allocator.
- **rect_pack uses the default allocator, not the arena.** `rp.init`/`rp.pack` allocate internally (node pool, results, tagged scratch) and free themselves via `delete`/`destroy`, which requires `context.allocator` to remain the default. Do NOT set `context.allocator = arena` globally — that would make rect_pack's internal `delete`s hit the arena and abort. Thread the arena only through our own explicit `allocator` params.
- `os.make_directory_all` returns `.Exist` when the dir already exists — treat that as success (both `atlas.odin` and `sprite_map.odin` already do).
- `flags.Help_Request` handles `-h`/`-help`; there is no separate `--help` flag.
- Sprite map format `auto` is inferred from the output file extension (`.json`/`.ssm`/`.c`/`.spr`); `structured-c` must be requested explicitly.
- Generated files (C/JSON/SSM) must escape filenames via the helpers in `util.odin`; text is emitted via `fmt.fprintf` where `{`/`}` need double braces.
- CLI-time string helpers (e.g. `heuristic_from_string`'s `strings.to_lower`) run before the pack arena exists; pass `context.temp_allocator` so they don't leak on the default allocator.
- **`--auto-size` probes** call `pack_sprites_skyline` repeatedly through the arena; each probe's `requests` slice accumulates in the arena and is reclaimed by the single `free_all` at the end (bounded, ~9 doublings max). The final probe leaves the correct offsets in the sprites.
