# SPR Sprite Bundle Format Specification

Version: 1

## Overview

The `.spr` file is a binary companion to a packed texture atlas (`.png`).
It stores the crop rectangles for every sprite that was packed into the
atlas, keyed by the FNV-1a 32-bit hash of the sprite's source PNG filename
(including its relative directory path from the project root).
The hash array is sorted ascending so that lookups use binary search at runtime.

Each texture atlas directory in `assets/` produces one `.spr` file in
`dist/`, named using a snake_case identifier derived from the directory
path (e.g. `assets/units/christina` → `dist/units_christina.spr`).

## File Layout

All multi-byte integers are **little-endian**.

```
Offset  Size          Field
──────  ────          ─────
 0      4             magic        {'S','P','R','S'}
 4      4             version      u32le (currently 1)
 8      1             scale_mode   u8    (see Scale Modes)
 9      3             padding      (implicit alignment to 4 bytes)
12      4             sprite_count u32le
16      sprite_count × 4   sprite_hashes   u32le[sprite_count]  (ascending)
16 + sprite_count × 4
      sprite_count × 8   sprite_crops    Packed_Sprite_Crop[sprite_count]
```

**Total header size:** 16 bytes (Odin struct packing pads `scale_mode`
to 4 bytes so `sprite_count` is naturally aligned).

## Header — `Sprite_Crop_Bundle_Header` (16 bytes)

| Offset | Field        | Type    | Description                                  |
|--------|--------------|---------|----------------------------------------------|
| 0      | `magic`      | [4]byte | Always `0x53 0x50 0x52 0x53` (`"SPRS"`)     |
| 4      | `version`    | u32le   | Format version (currently **1**)             |
| 8      | `scale_mode` | u8      | GPU texture scale mode (see below)           |
| 9-15   | —            | padding | Implicit 3-byte pad (Odin struct alignment)  |
| 12     | `sprite_count` | u32le | Number of sprites in this bundle            |

## Scale Modes (`Texture_Scale_Mode`, stored as u8)

| Value | Name      | SDL3 Equivalent |
|-------|-----------|-----------------|
| 0     | `Nearest` | `.NEAREST`      |
| 1     | `Linear`  | `.LINEAR`       |
| 2     | `Pixel_Art` | `.PIXELART`   |

## Sprite Hash Array

- **Type:** `u32le[sprite_count]`
- **Size:** `sprite_count × 4` bytes
- **Start offset:** 16 (immediately after header)

Each entry is the **FNV-1a 32-bit hash** of the sprite's source PNG
filename as an absolute path string (e.g. `"assets/units/christina/0001.png"`).

The hash is computed with Odin's `hash.fnv32a` over the raw byte
representation of the filename string.

Entries are sorted in **ascending** hash order to enable binary search
lookup at runtime (`slice.binary_search`).

## Sprite Crop Array — `Packed_Sprite_Crop` (8 bytes each)

- **Type:** `Packed_Sprite_Crop[sprite_count]`
- **Size:** `sprite_count × 8` bytes
- **Start offset:** `16 + sprite_count × 4` (immediately after hash array)

| Offset | Field | Type   | Description                                          |
|--------|-------|--------|------------------------------------------------------|
| 0      | `x`   | u16le  | Left position in pixels, relative to atlas top-left  |
| 2      | `y`   | u16le  | Top position in pixels, relative to atlas top-left   |
| 4      | `w`   | u16le  | Width in pixels                                      |
| 6      | `h`   | u16le  | Height in pixels                                     |

The crop array is in the **same order** as the hash array. Index `i` in
`sprite_hashes` corresponds to index `i` in `sprite_crops`.

Crops are content-rects with no padding (the rect-packer uses padding
during layout but the saved crop excludes it).

## Lookup Algorithm

Given a `Sprite_ID` (which is a `u32` FNV-1a hash of a sprite filename):

1. Read `sprite_count` from the header.
2. Binary search `sprite_hashes[0..sprite_count]` for the target hash.
3. If found at index `i`, the crop is `sprite_crops[i]`.
4. The crop coordinates are relative to the atlas PNG's top-left corner.

## Companion Files

Each `.spr` file has a matching atlas image:

| Source directory           | Atlas image                | Sprite bundle              |
|----------------------------|----------------------------|----------------------------|
| `assets/units/christina/`  | `dist/units_christina.png` | `dist/units_christina.spr` |
| `assets/gui/`              | `dist/gui.png`             | `dist/gui.spr`             |
| `assets/tiles/`            | `dist/tiles.png`           | `dist/tiles.spr`           |

## Notes

- The file is read with `read_entire_file_aligned(…, 64)` for safe
  pointer casting of the header and arrays directly from the mapped bytes.
- Both the hash array and crop array are cast from raw byte pointers and
  sliced by `sprite_count` — there is no per-entry framing or length prefix.
- The format is write-once at build time; no in-place editing is supported.
- Both the packer and the loader treat the struct memory layout as the wire
  format (no serialization layer).
