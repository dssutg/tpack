// sprites.rs - Sprite image discovery and loading.
//
// Responsible for recursively walking a directory for image files, decoding
// each into an RGBA surface, and managing their lifetimes. The Sprite struct
// combines the raw surface with its packing result (offset + packed flag).

use std::fs;
use std::path::Path;

use crate::atlas::Error;

// Surface is the decoded pixel buffer for one image (or the atlas).
//
// The pixels are stored in RGBA8 byte order, matching the tool's canonical
// format so pixel-copying later is a plain 32-bit copy. Pitch is always
// width * 4 because the buffer is tightly packed.
#[derive(Clone, Debug, Default)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA8 pixel data
}

impl Surface {
    // blank allocates a zero-filled surface of the given size.
    pub fn blank(width: u32, height: u32) -> Surface {
        Surface {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    // pitch returns the number of bytes per row.
    pub fn pitch(&self) -> usize {
        self.width as usize * 4
    }
}

// Sprite is a single image that will be packed into the atlas.
//
// offset holds the top-left pixel coordinate of the sprite's content within
// the final atlas (post-padding), and packed records whether it was
// successfully placed. A sprite that did not fit keeps offset zeroed and
// packed = false.
#[derive(Clone, Debug, Default)]
pub struct Sprite {
    pub offset: (i32, i32), // top-left position in the atlas
    pub image_path: String, // filesystem path the sprite was loaded from
    pub surface: Surface,   // decoded RGBA8 pixel data
    pub packed: bool,       // whether the sprite was placed in the atlas
}

// find_and_read_sprites discovers every file with the given extension under
// path (recursively) and decodes each into a Sprite, returning them as a
// single Vec.
//
// Sprite storage is owned by the returned Vec and released automatically when
// it is dropped. On error the Vec is empty and a descriptive error is
// returned.
pub fn find_and_read_sprites(path: &str, file_ext: &str) -> Result<Vec<Sprite>, Error> {
    let filenames = find_image_files(path, file_ext)
        .map_err(|e| Error::Message(format!("cannot read sprite image files: {e}")))?;

    let mut sprites = Vec::with_capacity(filenames.len());
    for file_path in filenames {
        match read_sprite_from_file(&file_path) {
            Ok(sprite) => sprites.push(sprite),
            Err(rerr) => {
                // Surface storage of already-loaded sprites is released when
                // the partially built Vec is dropped.
                return Err(Error::Message(format!(
                    "cannot read sprite image file {file_path}: {rerr}"
                )));
            }
        }
    }

    Ok(sprites)
}

// find_image_files recursively collects the paths of all files under
// dir_path whose name ends (case-insensitively) with extension.
pub fn find_image_files(dir_path: &str, extension: &str) -> std::io::Result<Vec<String>> {
    let mut paths = Vec::with_capacity(100);
    walk_image_dir(
        Path::new(dir_path),
        Path::new(dir_path),
        extension,
        &mut paths,
    )?;
    Ok(paths)
}

// walk_image_dir is the recursive core of find_image_files. It walks the
// directory rooted at root, appending matching file paths to out.
//
// subdirectories are visited depth-first and directory entries are sorted by
// name so the resulting list (and therefore the atlas layout) is
// deterministic across runs.
fn walk_image_dir(
    _root: &Path,
    dir: &Path,
    extension: &str,
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;

    // Sort entries so traversal order (and sprite order) is deterministic.
    entries.sort_by_key(|a| a.file_name());

    for e in entries {
        let file_type = e.file_type()?;

        if file_type.is_dir() {
            walk_image_dir(_root, &e.path(), extension, out)?;
        } else {
            // Match the extension case-insensitively so ".PNG" works too.
            let name = e.file_name();
            let name = name.to_string_lossy();
            if string_has_suffix_fold(&name, extension) {
                out.push(e.path().to_string_lossy().into_owned());
            }
        }
    }
    Ok(())
}

// string_has_suffix_fold reports whether the string s ends with suffix,
// compared case-insensitively.
fn string_has_suffix_fold(s: &str, suffix: &str) -> bool {
    let s = s.to_lowercase();
    let suffix = suffix.to_lowercase();
    s.len() >= suffix.len() && s.ends_with(&suffix)
}

// read_sprite_from_file decodes a single image file into a Sprite.
//
// The raw image is immediately converted to the tool's canonical RGBA8
// format so pixel-copying later is a plain 32-bit copy. The image_path string
// is owned by the returned Sprite along with the pixel buffer.
//
// The error is a plain string ("" on success) so the caller never has to
// decode a non-string error variant.
pub fn read_sprite_from_file(path: &str) -> Result<Sprite, String> {
    let raw = image::open(path).map_err(|e| format!("cannot load image {path}: {e}"))?;

    // Decode through 16-bit RGBA and keep the high byte of every channel.
    // This matches libpng/SDL_image's 16-to-8-bit truncation for 16-bit
    // source PNGs; image's direct to_rgba8 scaling can differ by one LSB.
    let rgba16 = raw.to_rgba16();
    let mut pixels = Vec::with_capacity(rgba16.len() * 4);
    for p in rgba16.pixels() {
        pixels.extend_from_slice(&[
            (p[0] >> 8) as u8,
            (p[1] >> 8) as u8,
            (p[2] >> 8) as u8,
            (p[3] >> 8) as u8,
        ]);
    }
    Ok(Sprite {
        image_path: path.to_string(),
        surface: Surface {
            width: rgba16.width(),
            height: rgba16.height(),
            pixels,
        },
        ..Default::default()
    })
}
