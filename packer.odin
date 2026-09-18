// packer.odin - Skyline rectangle packing and pixel compositing.
//
// Wraps the external rect_pack library (vendor:dss/rect_pack) to lay out
// sprites into the atlas bin, then blits each sprite's pixels into an SDL
// atlas surface.

package main

import "core:c"
import "core:fmt"
import "core:mem"

import rp "vendor:dss/rect_pack"
import sdl "vendor:sdl2"

// pack_sprites_skyline lays the given sprites out in a width x height bin
// using the requested rect_pack heuristic, writing each sprite's packed
// position into its offset field.
//
// padding is added to every request on each side so sprites never touch,
// preventing edge bleeding under bilinear filtering. The returned count is
// the number of sprites that fit; sprites[i].packed is true iff sprite i was
// placed. When the bin is too small the count is < len(sprites).
//
// The request slice is allocated in allocator (an arena) and reclaimed by the
// arena's free_all. rect_pack's own scratch (node pool, results, sorting
// scratch) is internally freed, so it uses the default allocator unchanged.
pack_sprites_skyline :: proc(
	sprites: []Sprite,
	atlas_width, atlas_height, padding: int,
	heuristic: rp.Heuristic,
	should_sort: bool,
	allocator: mem.Allocator,
) -> (
	packed: int,
) {
	packer: rp.Packer
	rp.init(&packer, atlas_width, atlas_height, len(sprites), heuristic)
	defer rp.destroy(&packer)

	// Build the request list, inflating each rect by padding on all sides.
	requests := make([]rp.Request, len(sprites), allocator)
	for s, i in sprites {
		requests[i] = rp.Request {
			w = int(s.surface.w) + padding * 2,
			h = int(s.surface.h) + padding * 2,
		}
	}

	// We already padded the requests; tell the packer not to add more.
	// Results are index-aligned with requests. results is internally
	// allocated/freed by rect_pack (default allocator), so it is deleted here.
	results, _ := rp.pack(&packer, requests, padding = 0, should_sort = should_sort)
	defer delete(results)

	for r, i in results {
		s := &sprites[i]
		if r.was_packed {
			// rect_pack already shifted x/y by +padding to point at the
			// content origin, so store these directly.
			s.offset = [2]int{r.x, r.y}
			s.packed = true
			packed += 1
		}
	}
	return
}

// copy_sprite_to_atlas composits the (src) sprite surface into the (dst)
// atlas at pixel offset (ox, oy).
//
// The compositing is a manual pixel loop rather than an SDL blit because a
// fully transparent texel must not carry its source RGB. PNGs routinely store
// leftover color data (often black) in alpha-0 pixels; if that reached the
// atlas it would show up as a dark fringe around sprites when the atlas is
// sampled with linear filtering. SDL's blitters cannot express "force RGB to 0
// where alpha is 0": a straight copy (Blend_Mode.NONE) keeps the source color
// and a blended copy (Blend_Mode.BLEND) keeps the destination color. So the
// alpha-0 case is handled explicitly, writing transparent black.
//
// The blit is clipped to the atlas bounds: if the sprite extends past the
// right or bottom edge the out-of-range portion is dropped rather than
// written out of bounds (defensive: packing should keep every sprite inside,
// but a bug in the packer must not corrupt memory).
copy_sprite_to_atlas :: proc(src, dst: ^sdl.Surface, ox, oy: int) {
	src_pixels := ([^]u8)(src.pixels)
	dst_pixels := ([^]u8)(dst.pixels)
	src_pitch := int(src.pitch)
	dst_pitch := int(dst.pitch)
	w := int(src.w)
	h := int(src.h)
	dst_w := int(dst.w)
	dst_h := int(dst.h)

	// Clip the copy region to the intersection of the sprite and the atlas.
	sx0 := max(0, ox)
	sy0 := max(0, oy)
	sx1 := min(dst_w, ox + w)
	sy1 := min(dst_h, oy + h)
	if sx0 >= sx1 || sy0 >= sy1 {
		return
	}

	for y in sy0 ..< sy1 {
		for x in sx0 ..< sx1 {
			// Source and destination byte offsets (4 bytes per pixel).
			si := (y - oy) * src_pitch + (x - ox) * 4
			di := y * dst_pitch + x * 4
			a := src_pixels[si + 3]
			if a == 0 {
				// Transparent: clear to transparent black.
				dst_pixels[di + 0] = 0
				dst_pixels[di + 1] = 0
				dst_pixels[di + 2] = 0
				dst_pixels[di + 3] = 0
			} else {
				// Copy the BGRA channels verbatim.
				dst_pixels[di + 0] = src_pixels[si + 0]
				dst_pixels[di + 1] = src_pixels[si + 1]
				dst_pixels[di + 2] = src_pixels[si + 2]
				dst_pixels[di + 3] = a
			}
		}
	}
}

// create_atlas_surface allocates a blank SDL surface sized to the atlas in
// the tool's canonical ABGR8888 format. Returns nil on failure.
//
// allocator backs error strings so they live on the same arena as every
// other pack-cycle allocation (see AGENTS.md "Arena memory model").
create_atlas_surface :: proc(
	width, height: int,
	allocator: mem.Allocator,
) -> (
	atlas: ^sdl.Surface,
	err: Error,
) {
	atlas = sdl.CreateRGBSurfaceWithFormat(
		flags = 0,
		width = c.int(width),
		height = c.int(height),
		depth = 32,
		format = u32(sdl.PixelFormatEnum.ABGR8888),
	)
	if atlas == nil {
		return nil, fmt.aprintf(
			"failed to create atlas surface: %s",
			sdl.GetErrorString(),
			allocator = allocator,
		)
	}
	return
}
