// sprite_map.rs - Sprite map output generation (C, JSON, SSM, SPR).
//
// save_atlas_sprite_map writes a description of where each sprite landed in
// the atlas, in one of several formats. Each format is deliberately
// self-contained and deterministic.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use crate::atlas::Error;
use crate::config::{ext_to_map_format, Map_Format, Spr_Scale_Mode};
use crate::sprites::Sprite;
use crate::util::{escape_json_string, escape_ssm_field};

// Generated-file banner. Every format that emits source code starts with a
// comment warning that the file is generated, so it is not hand-edited.
pub const generated_banner: &str = "/* DO NOT EDIT; generated with tpack */\n\n";

// resolve_map_format turns a possibly-.Auto Map_Format into a concrete one by
// inspecting the output file extension. It is called up front, before anything
// is written, so an unresolvable extension fails without truncating an existing
// output file or leaving a fresh atlas PNG with no map behind.
pub fn resolve_map_format(format: Map_Format, sprite_map_path: &str) -> Result<Map_Format, Error> {
    if format != Map_Format::Auto {
        return Ok(format);
    }
    let ext = path_ext(sprite_map_path).to_lowercase();
    if let Some(f) = ext_to_map_format(&ext) {
        return Ok(f);
    }
    Err(Error::Message(format!(
        "cannot determine sprite map format from extension of {sprite_map_path}"
    )))
}

// save_atlas_sprite_map writes the sprite map to sprite_map_path, creating
// the parent directory if needed. format must already be concrete (the caller
// resolves .Auto via resolve_map_format before any file is opened, so an
// unresolvable extension never truncates an existing output). The actual map
// body is streamed to disk, so it does not accumulate in memory.
//
// spr_root and spr_scale_mode are used only by the binary .spr writer: the
// former is the project root that sprite path hashes are made relative to,
// and the latter is recorded in the spr header's scale_mode byte.
#[allow(clippy::too_many_arguments)]
pub fn save_atlas_sprite_map(
    format: Map_Format,
    sprite_map_path: &str,
    map_enum_prefix: &str,
    map_array_name: &str,
    map_array_type: &str,
    sprites: &[Sprite],
    spr_root: &str,
    spr_scale_mode: Spr_Scale_Mode,
) -> Result<(), Error> {
    // Create parent directories (ignore "already exists").
    if let Some(dir) = Path::new(sprite_map_path).parent() {
        if !dir.as_os_str().is_empty() && dir != Path::new(".") {
            if let Err(merr) = fs::create_dir_all(dir) {
                return Err(Error::Message(format!(
                    "cannot create directory for sprite map: {merr}"
                )));
            }
        }
    }

    // Open (truncating) the output file: 0644 permissions.
    let mut open = OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o644);
    }
    let mut handle = open.open(sprite_map_path).map_err(|oerr| {
        Error::Message(format!(
            "error creating sprite map file {sprite_map_path}: {oerr}"
        ))
    })?;

    match format {
        Map_Format::JSON => {
            let _ = write_json_sprite_map(&mut handle, sprites);
        }
        Map_Format::SSM => {
            let _ = write_ssm_sprite_map(&mut handle, sprites);
        }
        Map_Format::C => {
            let _ = write_c_sprite_map(&mut handle, map_array_name, sprites);
        }
        Map_Format::Structured_C => {
            let _ = write_structured_c_sprite_map(
                &mut handle,
                map_enum_prefix,
                map_array_name,
                map_array_type,
                sprites,
            );
        }
        Map_Format::SPR => {
            write_spr_sprite_map(&mut handle, sprites, spr_root, spr_scale_mode)?;
        }
        Map_Format::Auto => {
            return Err(Error::Message(format!(
                "cannot determine sprite map format from extension of {sprite_map_path}"
            )));
        }
    }
    Ok(())
}

// write_json_sprite_map writes an object keyed by sprite image path:
//
//	{
//	  "path/to/a.png": { "x": 0, "y": 0, "w": 16, "h": 16 },
//	  "path/to/b.png": { "x": 16, "y": 0, "w": 32, "h": 32 }
//	}
//
// Keys are JSON-escaped. Valid, parseable JSON is guaranteed for any input.
fn write_json_sprite_map(handle: &mut File, sprites: &[Sprite]) -> io::Result<()> {
    handle.write_all(b"{\n")?;
    for (i, s) in sprites.iter().enumerate() {
        let escaped = escape_json_string(&s.image_path);
        write!(
            handle,
            "  \"{}\": {{ \"x\": {}, \"y\": {}, \"w\": {}, \"h\": {} }}",
            escaped, s.offset.0, s.offset.1, s.surface.width, s.surface.height
        )?;
        if i != sprites.len() - 1 {
            handle.write_all(b",")?;
        }
        handle.write_all(b"\n")?;
    }
    handle.write_all(b"}\n")?;
    Ok(())
}

// write_ssm_sprite_map writes one tab-separated record per sprite:
//
//	S\t<path>\t<x>\t<y>\t<w>\t<h>
//
// The "S" tag marks a sprite record. Field values are escaped so tabs /
// newlines inside a path cannot corrupt the record framing.
fn write_ssm_sprite_map(handle: &mut File, sprites: &[Sprite]) -> io::Result<()> {
    for s in sprites {
        let escaped = escape_ssm_field(&s.image_path);
        writeln!(
            handle,
            "S\t{}\t{}\t{}\t{}\t{}",
            escaped, s.offset.0, s.offset.1, s.surface.width, s.surface.height
        )?;
    }
    Ok(())
}

// write_c_sprite_map writes a plain C source file containing a struct and an
// array of {filename, x, y, w, h} entries:
//
//	const struct { const char *filename; int x, y, w, h; } sprites[] = {
//	  { .filename = "...", .x = 0, .y = 0, .w = 16, .h = 16 },
//	};
fn write_c_sprite_map(
    handle: &mut File,
    map_array_name: &str,
    sprites: &[Sprite],
) -> io::Result<()> {
    handle.write_all(generated_banner.as_bytes())?;
    handle.write_all(b"const struct {\n")?;
    handle.write_all(b"  const char *filename;\n")?;
    handle.write_all(b"  int x, y, w, h;\n")?;
    writeln!(handle, "}} {}[] = {{", map_array_name)?;
    for s in sprites {
        let escaped = escape_json_string(&s.image_path);
        writeln!(
            handle,
            "  {{ .filename = \"{}\", .x = {}, .y = {}, .w = {}, .h = {} }},",
            escaped, s.offset.0, s.offset.1, s.surface.width, s.surface.height
        )?;
    }
    handle.write_all(b"};\n")?;
    Ok(())
}

// write_structured_c_sprite_map writes a C file with a generated enum of
// sprite identifiers plus an optionally-instantiated array indexed by those
// enum values:
//
//	enum { SPRITE_Air, SPRITE_Aux1Btn, ... };
//	extern const SDL_Rect sprites[];
//	#ifdef SPRITE_IMPLEMENTATION
//	const SDL_Rect sprites[] = { [SPRITE_Air] = { .x = ..., }, ... };
//	#endif
//
// The array body is guarded by <prefix>_IMPLEMENTATION so callers include the
// header for declarations and define it once to get the definition.
fn write_structured_c_sprite_map(
    handle: &mut File,
    map_enum_prefix: &str,
    map_array_name: &str,
    map_array_type: &str,
    sprites: &[Sprite],
) -> io::Result<()> {
    handle.write_all(generated_banner.as_bytes())?;
    handle.write_all(b"enum {\n")?;
    for s in sprites {
        writeln!(
            handle,
            "  {}{},",
            map_enum_prefix,
            image_path_to_enum_name(&s.image_path)
        )?;
    }
    handle.write_all(b"};\n\n")?;
    writeln!(
        handle,
        "extern const {} {}[];",
        map_array_type, map_array_name
    )?;
    handle.write_all(b"\n")?;
    writeln!(handle, "#ifdef {}_IMPLEMENTATION", map_enum_prefix)?;
    writeln!(handle, "const {} {}[] = {{", map_array_type, map_array_name)?;
    for s in sprites {
        writeln!(
            handle,
            " [{}{}] = {{ .x = {}, .y = {}, .w = {}, .h = {} }},",
            map_enum_prefix,
            image_path_to_enum_name(&s.image_path),
            s.offset.0,
            s.offset.1,
            s.surface.width,
            s.surface.height
        )?;
    }
    handle.write_all(b"};\n")?;
    writeln!(handle, "#endif /* {}_IMPLEMENTATION */", map_enum_prefix)?;
    Ok(())
}

// image_path_to_enum_name derives a C identifier from an image filename by
// removing the extension, splitting on '-', '_' and '.', and capitalizing each
// word. Examples:
//
//	"aux1_btn.png"       -> "Aux1Btn"
//	"minimap_mask_round" -> "MinimapMaskRound"
//
// The result is suitable to append after the user's enum prefix.
pub fn image_path_to_enum_name(name: &str) -> String {
    // Rebuild the base name one delimiter-separated word at a time.
    let mut out = String::new();

    let base_name = Path::new(name)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    let base_name = remove_filename_extension(&base_name);
    let mut part_idx = 0;
    for part in base_name.split(['/', '-', '.', '_', ' ']) {
        if part.is_empty() {
            // Ignore empty parts left behind by multiple delimiters.
            continue;
        }

        // Insert underscore only if not the first part.
        if part_idx > 0 {
            out.push('_');
        }
        part_idx += 1;

        // Capitalize the first rune and keep the rest as-is.
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }

    out
}

// .spr binary sprite bundle

// Spr_Magic is the 4-byte "SPRS" magic that begins every .spr file.
const Spr_Magic: [u8; 4] = *b"SPRS";

// Spr_Version is the current .spr format version.
const Spr_Version: u32 = 1;

// The .spr header (see docs/spr-format-spec.md) is a fixed 16-byte layout.
// The little-endian field types fix the byte layout; sprite_count is aligned
// to offset 12, leaving 3 implicit padding bytes after scale_mode.
const SPR_HEADER_SIZE: usize = 16;
const SPR_HASH_SIZE: usize = 4;
const SPR_CROP_SIZE: usize = 8;

// spr_crop holds one 8-byte little-endian content rectangle (no padding).
#[derive(Clone, Copy, Debug, Default)]
struct spr_crop {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

// spr_entry pairs a sprite's FNV-1a path hash with its crop, for sorting.
#[derive(Clone, Copy, Debug, Default)]
struct spr_entry {
    hash: u32,
    crop: spr_crop,
}

// spr_rel_path returns the path string to hash for a sprite. When spr_root is
// non-empty and is a prefix of image_path, the leading root is stripped so the
// hash is project-root-relative (e.g. "assets/units/christina/0001.png").
// The returned substring points into image_path (no allocation).
fn spr_rel_path<'a>(image_path: &'a str, spr_root: &str) -> &'a str {
    if !spr_root.is_empty() && image_path.starts_with(spr_root) {
        let mut rel = &image_path[spr_root.len()..];
        if let Some(first) = rel.as_bytes().first() {
            if *first == b'/' || *first == b'\\' {
                rel = &rel[1..];
            }
        }
        return rel;
    }
    image_path
}

// write_spr_sprite_map writes a binary .spr sprite bundle. The file is the
// in-memory layout of the header, followed by a u32le hash per sprite and a
// crop per sprite (see docs/spr-format-spec.md); the little-endian field
// layout defines the byte layout, so the whole bundle is staged in one buffer
// and written with a single write.
//
// Hashes cover the sprite's path relative to spr_root (or the full path when
// no root is given), so the hash is project-wide unique and implies the full
// relative filename. They are emitted in ascending order for binary search.
fn write_spr_sprite_map(
    handle: &mut File,
    sprites: &[Sprite],
    spr_root: &str,
    scale_mode: Spr_Scale_Mode,
) -> Result<(), Error> {
    let count = sprites.len();

    // Pair each sprite's hash with its content crop (offset already excludes
    // padding and surface w/h are the unpadded dimensions).
    let mut entries: Vec<spr_entry> = Vec::with_capacity(count);
    for s in sprites {
        let rel = spr_rel_path(&s.image_path, spr_root);
        entries.push(spr_entry {
            hash: fnv32a(rel.as_bytes()),
            crop: spr_crop {
                x: s.offset.0.clamp(0, 0xFFFF) as u16,
                y: s.offset.1.clamp(0, 0xFFFF) as u16,
                w: (s.surface.width as i32).clamp(0, 0xFFFF) as u16,
                h: (s.surface.height as i32).clamp(0, 0xFFFF) as u16,
            },
        });
    }

    // Sort ascending so lookups can use binary search at runtime.
    entries.sort_by_key(|a| a.hash);

    // Stage the whole file in memory: header, then hashes, then crops.
    let data_len = SPR_HEADER_SIZE + count * (SPR_HASH_SIZE + SPR_CROP_SIZE);
    let mut data = vec![0u8; data_len];

    // Write the header directly into the buffer, field by field. The zeroed
    // buffer and per-field writes never touch the header's implicit padding
    // (bytes 9..11, after scale_mode), so it stays zero - no hardcoded byte
    // offsets, and the .spr output is byte-deterministic across runs.
    data[0..4].copy_from_slice(&Spr_Magic);
    data[4..8].copy_from_slice(&Spr_Version.to_le_bytes());
    data[8] = scale_mode as u8;
    // data[9..12] stays zero (implicit alignment padding).
    data[12..16].copy_from_slice(&(count as u32).to_le_bytes());

    let hashes_start = SPR_HEADER_SIZE;
    let crops_start = SPR_HEADER_SIZE + count * SPR_HASH_SIZE;
    for (i, e) in entries.iter().enumerate() {
        let ho = hashes_start + i * SPR_HASH_SIZE;
        data[ho..ho + SPR_HASH_SIZE].copy_from_slice(&e.hash.to_le_bytes());
        let co = crops_start + i * SPR_CROP_SIZE;
        data[co..co + 2].copy_from_slice(&e.crop.x.to_le_bytes());
        data[co + 2..co + 4].copy_from_slice(&e.crop.y.to_le_bytes());
        data[co + 4..co + 6].copy_from_slice(&e.crop.w.to_le_bytes());
        data[co + 6..co + 8].copy_from_slice(&e.crop.h.to_le_bytes());
    }

    match handle.write(&data) {
        Ok(n) if n == data.len() => Ok(()),
        Ok(n) => Err(Error::Message(format!(
            "error writing sprite map file: short write ({n} of {} bytes)",
            data.len()
        ))),
        Err(werr) => Err(Error::Message(format!(
            "error writing sprite map file: {werr}"
        ))),
    }
}

// fnv32a computes the 32-bit FNV-1a hash of a byte string.
fn fnv32a(bytes: &[u8]) -> u32 {
    let mut hash = 2_166_136_261u32;
    for &b in bytes {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

// path_ext returns the extension of path, including its leading dot, or "".
fn path_ext(path: &str) -> String {
    match Path::new(path).extension() {
        Some(ext) => format!(".{}", ext.to_string_lossy()),
        None => String::new(),
    }
}

// remove_filename_extension returns the filename without its file extension.
fn remove_filename_extension(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(i) => &filename[..i],
        None => filename,
    }
}
