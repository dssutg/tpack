// sprite_map.rs - Sprite map output generation (C, JSON, SSM, SPR).
//
// save_atlas_sprite_map writes a description of where each sprite landed in
// the atlas, in one of several formats. Each format is deliberately
// self-contained and deterministic.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use tpack_hash::fnv32a;


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

// Spr_Header is the declarative on-disk header layout. Field order and types
// define the byte layout; multi-byte values are stored little-endian. The
// explicit padding field keeps the struct free of implicit padding, so it can
// be serialized directly with bytemuck.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Spr_Header {
    magic: [u8; 4],
    version: [u8; 4],
    scale_mode: u8,
    padding: [u8; 3],
    sprite_count: [u8; 4],
}

// Packed_Sprite_Crop is the declarative on-disk crop layout: four little-
// endian u16 values, in x/y/w/h order, with no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Packed_Sprite_Crop {
    x: [u8; 2],
    y: [u8; 2],
    w: [u8; 2],
    h: [u8; 2],
}

// bytemuck safety: both structs are repr(C), contain only byte arrays, and
// have no padding or invalid bit patterns.
unsafe impl Zeroable for Spr_Header {}
unsafe impl Pod for Spr_Header {}
unsafe impl Zeroable for Packed_Sprite_Crop {}
unsafe impl Pod for Packed_Sprite_Crop {}

impl Spr_Header {
    fn new(scale_mode: u8, sprite_count: u32) -> Self {
        Spr_Header {
            magic: Spr_Magic,
            version: Spr_Version.to_le_bytes(),
            scale_mode,
            padding: [0; 3],
            sprite_count: sprite_count.to_le_bytes(),
        }
    }
}

impl Packed_Sprite_Crop {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Packed_Sprite_Crop {
            x: (x.clamp(0, 0xFFFF) as u16).to_le_bytes(),
            y: (y.clamp(0, 0xFFFF) as u16).to_le_bytes(),
            w: (w.clamp(0, 0xFFFF) as u16).to_le_bytes(),
            h: (h.clamp(0, 0xFFFF) as u16).to_le_bytes(),
        }
    }
}

const SPR_HEADER_SIZE: usize = std::mem::size_of::<Spr_Header>();
const SPR_HASH_SIZE: usize = std::mem::size_of::<u32>();
const SPR_CROP_SIZE: usize = std::mem::size_of::<Packed_Sprite_Crop>();

// The on-disk layout requires exactly these sizes (see docs/spr-format-spec.md).
const _: () = assert!(SPR_HEADER_SIZE == 16);
const _: () = assert!(SPR_CROP_SIZE == 8);

// spr_entry pairs a sprite's FNV-1a path hash with its crop, for sorting.
#[derive(Clone, Copy, Debug, Default)]
struct spr_entry {
    hash: u32,
    crop: Packed_Sprite_Crop,
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
            crop: Packed_Sprite_Crop::new(
                s.offset.0,
                s.offset.1,
                s.surface.width as i32,
                s.surface.height as i32,
            ),
        });
    }

    // Sort ascending so lookups can use binary search at runtime.
    entries.sort_by_key(|a| a.hash);

    // Stage the whole file in memory: header, then hashes, then crops. The
    // structs above define each field's position, so this code never spells
    // out byte offsets; bytemuck serializes the declarations directly.
    let header = Spr_Header::new(scale_mode as u8, count as u32);
    let data_len = SPR_HEADER_SIZE + count * (SPR_HASH_SIZE + SPR_CROP_SIZE);
    let mut data = Vec::with_capacity(data_len);
    data.extend_from_slice(bytes_of(&header));
    for e in &entries {
        data.extend_from_slice(&e.hash.to_le_bytes());
    }
    let crops: Vec<Packed_Sprite_Crop> = entries.iter().map(|e| e.crop).collect();
    data.extend_from_slice(cast_slice(&crops));

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
