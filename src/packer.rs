// packer.rs - Skyline rectangle packing and pixel compositing.
//
// Wraps the rect_pack library (lib/rect_pack) to lay out sprites into the
// atlas bin, then blits each sprite's pixels into an RGBA atlas surface.

use rect_pack as rp;

use crate::atlas::Error;
use crate::sprites::{Sprite, Surface};

// pack_sprites_skyline lays the given sprites out in a width x height bin
// using the requested rect_pack heuristic, writing each sprite's packed
// position into its offset field.
//
// padding is added to every request on each side so sprites never touch,
// preventing edge bleeding under bilinear filtering. The returned count is
// the number of sprites that fit; sprites[i].packed is true iff sprite i was
// placed. When the bin is too small the count is < sprites.len().
//
// The request Vec is owned by this function and released when it goes out of
// scope; rect_pack owns its internal scratch as well.
pub fn pack_sprites_skyline(
    sprites: &mut [Sprite],
    atlas_width: i32,
    atlas_height: i32,
    padding: i32,
    heuristic: rp::Heuristic,
    should_sort: bool,
) -> usize {
    let mut packer = rp::Packer::default();
    packer.init(atlas_width, atlas_height, sprites.len() as i32, heuristic);

    // Build the request list, inflating each rect by padding on all sides.
    let requests: Vec<rp::Request> = sprites
        .iter()
        .map(|s| rp::Request {
            w: s.surface.width as i32 + padding * 2,
            h: s.surface.height as i32 + padding * 2,
        })
        .collect();

    // We already padded the requests; tell the packer not to add more.
    // Results are index-aligned with requests.
    let (results, _) = packer.pack(&requests, 0, should_sort);
    let mut packed = 0;

    for (s, r) in sprites.iter_mut().zip(results.iter()) {
        if r.was_packed {
            // rect_pack already shifted x/y by +padding to point at the
            // content origin, so store these directly.
            s.offset = (r.x, r.y);
            s.packed = true;
            packed += 1;
        }
    }
    packed
}

// copy_sprites_to_atlas composites every packed sprite into the atlas.
//
// Sprites are independent, so the atlas is divided into horizontal row bands
// and each band is filled by its own worker thread. Bands never share rows,
// so a sprite that crosses a band boundary is simply clipped and copied by
// each band that covers it. The resulting pixels are identical to copying the
// sprites one at a time.
pub fn copy_sprites_to_atlas(sprites: &[Sprite], atlas: &mut Surface, threads: usize) {
    let dst_w = atlas.width as i32;
    let dst_h = atlas.height as i32;
    let height = atlas.height as usize;
    let pitch = atlas.pitch();
    let workers = threads.max(1).min(height);

    if workers <= 1 {
        for s in sprites {
            copy_sprite_to_atlas(&s.surface, atlas, s.offset.0, s.offset.1);
        }
        return;
    }

    let band_rows = height.div_ceil(workers);
    let band_bytes = band_rows * pitch;
    let pixels = &mut atlas.pixels;
    std::thread::scope(|scope| {
        for (band_index, band) in pixels.chunks_mut(band_bytes).enumerate() {
            let band_y0 = (band_index * band_rows) as i32;
            let band_y1 = ((band_index + 1) * band_rows).min(height) as i32;
            scope.spawn(move || {
                for s in sprites {
                    copy_sprite_to_band(
                        &s.surface,
                        band,
                        dst_w,
                        dst_h,
                        (band_y0, band_y1),
                        (s.offset.0, s.offset.1),
                    );
                }
            });
        }
    });
}

// copy_sprite_to_atlas composits the (src) sprite surface into the (dst)
// atlas at pixel offset (ox, oy).
//
// The compositing is a manual pixel loop rather than a blit because a
// fully transparent texel must not carry its source RGB. PNGs routinely store
// leftover color data (often black) in alpha-0 pixels; if that reached the
// atlas it would show up as a dark fringe around sprites when the atlas is
// sampled with linear filtering. A straight copy keeps the source color and a
// blended copy keeps the destination color. So the alpha-0 case is handled
// explicitly, writing transparent black.
//
// The blit is clipped to the atlas bounds: if the sprite extends past the
// right or bottom edge the out-of-range portion is dropped rather than
// written out of bounds (defensive: packing should keep every sprite inside,
// but a bug in the packer must not corrupt memory).
pub fn copy_sprite_to_atlas(src: &Surface, dst: &mut Surface, ox: i32, oy: i32) {
    let dst_w = dst.width as i32;
    let dst_h = dst.height as i32;
    copy_sprite_to_band(src, &mut dst.pixels, dst_w, dst_h, (0, dst_h), (ox, oy));
}

// copy_sprite_to_band writes the portion of src that falls inside the atlas
// rows [band_y0, band_y1) into dst, whose first byte is that band's first
// pixel. dst_w and dst_h describe the full atlas, so clipping still happens
// against the real atlas bounds even though only one band is writable.
fn copy_sprite_to_band(
    src: &Surface,
    dst: &mut [u8],
    dst_w: i32,
    dst_h: i32,
    (band_y0, band_y1): (i32, i32),
    (ox, oy): (i32, i32),
) {
    let src_pitch = src.pitch();
    let dst_pitch = dst_w as usize * 4;
    let src_pixels = &src.pixels;
    let w = src.width as i32;
    let h = src.height as i32;

    // Clip the copy region to the intersection of the sprite, the atlas, and
    // this band.
    let sx0 = 0.max(ox);
    let sy0 = 0.max(oy).max(band_y0);
    let sx1 = dst_w.min(ox + w);
    let sy1 = dst_h.min(oy + h).min(band_y1);
    if sx0 >= sx1 || sy0 >= sy1 {
        return;
    }

    for y in sy0..sy1 {
        for x in sx0..sx1 {
            // Source and destination byte offsets (4 bytes per pixel).
            let si = (y - oy) as usize * src_pitch + (x - ox) as usize * 4;
            let di = (y - band_y0) as usize * dst_pitch + x as usize * 4;
            let a = src_pixels[si + 3];
            if a == 0 {
                // Transparent: clear to transparent black.
                dst[di] = 0;
                dst[di + 1] = 0;
                dst[di + 2] = 0;
                dst[di + 3] = 0;
            } else {
                // Copy the BGRA channels verbatim.
                dst[di] = src_pixels[si];
                dst[di + 1] = src_pixels[si + 1];
                dst[di + 2] = src_pixels[si + 2];
                dst[di + 3] = a;
            }
        }
    }
}

// create_atlas_surface allocates a blank surface sized to the atlas in
// the tool's canonical RGBA8 format. Returns an error on failure.
//
// Surface storage is owned by the returned value (see AGENTS.md).
pub fn create_atlas_surface(width: i32, height: i32) -> Result<Surface, Error> {
    if width <= 0 || height <= 0 {
        return Err(Error::Message(format!(
            "failed to create atlas surface: invalid size {width}x{height}"
        )));
    }
    Ok(Surface::blank(width as u32, height as u32))
}
