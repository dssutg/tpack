// incremental.odin - Incremental build support.
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

package main

import "core:fmt"
import "core:os"
import "core:path/filepath"
import "core:time"

// outputs_are_current reports whether both output files exist and are at least
// as new as every source file under sprites_dir. When it returns true, the
// atlas and sprite map are considered up to date and packing can be skipped.
//
// The returned error is set only when the sprite tree cannot be walked; a
// missing output simply means "not up to date" (a rebuild is required).
outputs_are_current :: proc(
	sprites_dir:     string,
	atlas_path:      string,
	sprite_map_path: string,
) -> (current: bool, err: Error) {
	atlas_mtime, aerr := os.modification_time_by_path(atlas_path)
	if aerr != nil {
		return false, nil
	}
	map_mtime, merr := os.modification_time_by_path(sprite_map_path)
	if merr != nil {
		return false, nil
	}

	newest_src, dir_err := newest_sprite_mtime(sprites_dir)
	if dir_err != nil {
		err = fmt.aprintf(
			"cannot stat sprite directory %s: %v",
			sprites_dir,
			os.error_string(dir_err),
			allocator = context.temp_allocator,
		)
		return false, err
	}

	// Rebuild if the newest source is newer than EITHER output, so a change
	// that lands in only one of the two outputs still forces a repack.
	atlas_t := time.to_unix_nanoseconds(atlas_mtime)
	map_t := time.to_unix_nanoseconds(map_mtime)
	oldest_out := min(atlas_t, map_t)

	return newest_src <= oldest_out, nil
}

// newest_sprite_mtime returns the newest modification time (in Unix
// nanoseconds) among every entry under root: the root directory itself, every
// subdirectory, and every file. Directories are counted so that adding,
// removing, or renaming a sprite — which updates only the parent directory's
// mtime, not the remaining files — is still detected as a change.
//
// If the tree cannot be walked, a non-nil error is returned.
newest_sprite_mtime :: proc(root: string) -> (newest: i64, err: os.Error) {
	mtime, rerr := os.modification_time_by_path(root)
	if rerr != nil {
		return 0, rerr
	}
	mtime_max := time.to_unix_nanoseconds(mtime)
	if werr := walk_mtime(root, &mtime_max); werr != nil {
		return 0, werr
	}
	return mtime_max, nil
}

// walk_mtime updates best with the newest modification time found anywhere
// under dir (files and subdirectories alike). Returns a non-nil error if the
// directory cannot be read.
walk_mtime :: proc(dir: string, best: ^i64) -> os.Error {
	h, oerr := os.open(dir)
	if oerr != nil {
		return oerr
	}
	defer os.close(h)

	entries, rerr := os.read_dir(h, 0, context.temp_allocator)
	if rerr != nil {
		return rerr
	}

	for e in entries {
		if t := time.to_unix_nanoseconds(e.modification_time); t > best^ {
			best^ = t
		}
		if e.type == .Directory {
			child, _ := filepath.join({dir, e.name}, context.temp_allocator)
			if werr := walk_mtime(child, best); werr != nil {
				return werr
			}
		}
	}
	return nil
}
