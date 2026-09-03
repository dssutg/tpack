# tpack

Odin texture packing CLI. Packs `.png` sprites into a texture atlas and generates sprite map output (C, JSON, SSM).

## Build and Run

```
odin run . <sprites_dir> <atlas_path> <sprite_map_path> [--flags]
```

- Build only: `odin build .`
- Run tests: `odin test .`

## External Dependency

Imports `../dssodinlib/rect_pack` (relative path). Must exist at that sibling path or build will fail.

## Test Harness

`test/pack/pack.odin` is a wrapper that runs `odin run .` with test sprite fixtures. Not an Odin test suite — it's a process-spawning harness.

## Code Structure

Single-file tool: `main.odin` (package `main`). No packages, no modules, no configs beyond Odin itself. Key functions:
- `pack_sprites_to_atlas` — orchestrator
- `walk_image_dir` — recursive PNG discovery
- `pack_sprites_skyline` — rect packing via dssodinlib
- `save_atlas_sprite_map` — output generation (C/JSON/SSM formats)
