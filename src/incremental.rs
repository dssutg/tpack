// incremental.rs - Incremental build support.
//
// With --incremental, the tool compares each built artifact (the atlas PNG and
// the sprite map file) against the newest source sprite and skips the rebuild
// entirely when the outputs are already newer than every source. This mirrors
// lone-colony's incremental_pack_sprites: both output files must exist AND be
// newer than every source sprite, otherwise the atlas is regenerated.
//
// Config (atlas size, padding, map format, ...) is assumed not to change
// between runs; stale detection is based purely on filesystem modification
// times, so only source edits trigger a rebuild.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::atlas::Error;

// outputs_are_current reports whether both output files exist and are at least
// as new as every source file under sprites_dir. When it returns true, the
// atlas and sprite map are considered up to date and packing can be skipped.
//
// The returned error is set only when the sprite tree cannot be walked; a
// missing output simply means "not up to date" (a rebuild is required).
pub fn outputs_are_current(
    sprites_dir: &str,
    atlas_path: &str,
    sprite_map_path: &str,
) -> Result<bool, Error> {
    let atlas_mtime = match mtime_nanos(Path::new(atlas_path)) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let map_mtime = match mtime_nanos(Path::new(sprite_map_path)) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };

    let newest_src = newest_sprite_mtime(sprites_dir)
        .map_err(|e| Error::Message(format!("cannot stat sprite directory {sprites_dir}: {e}")))?;

    // Rebuild if the newest source is newer than EITHER output, so a change
    // that lands in only one of the two outputs still forces a repack.
    let oldest_out = atlas_mtime.min(map_mtime);

    // Strict "newer than": a source with the SAME timestamp as an output is
    // stale. On coarse-granularity filesystems (the output and source written
    // within the same tick) `<=` would wrongly mark an edit as current.
    Ok(newest_src < oldest_out)
}

// newest_sprite_mtime returns the newest modification time (in Unix
// nanoseconds) among every entry under root: the root directory itself, every
// subdirectory, and every file. Directories are counted so that adding,
// removing, or renaming a sprite — which updates only the parent directory's
// mtime, not the remaining files — is still detected as a change.
//
// If the tree cannot be walked, a non-nil error is returned.
fn newest_sprite_mtime(root: &str) -> std::io::Result<u128> {
    let mut newest = mtime_nanos(Path::new(root))?;
    walk_mtime(Path::new(root), &mut newest)?;
    Ok(newest)
}

// walk_mtime updates best with the newest modification time found anywhere
// under dir (files and subdirectories alike). Returns a non-nil error if the
// directory cannot be read.
fn walk_mtime(dir: &Path, best: &mut u128) -> std::io::Result<()> {
    let entries = fs::read_dir(dir)?;

    for e in entries {
        let e = e?;
        let meta = e.metadata()?;
        if let Ok(t) = mtime_nanos_from(meta.modified()) {
            if t > *best {
                *best = t;
            }
        }
        if e.file_type()?.is_dir() {
            walk_mtime(&e.path(), best)?;
        }
    }
    Ok(())
}

// mtime_nanos returns a path's modification time in Unix nanoseconds.
fn mtime_nanos(path: &Path) -> std::io::Result<u128> {
    mtime_nanos_from(fs::metadata(path)?.modified())
}

// mtime_nanos_from converts a SystemTime to Unix nanoseconds.
fn mtime_nanos_from(t: std::io::Result<SystemTime>) -> std::io::Result<u128> {
    Ok(t?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0))
}
