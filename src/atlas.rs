// atlas.rs - Top-level packing orchestration and atlas PNG output.
//
// pack_sprites_to_atlas is the main entry point of the tool: it validates
// options, discovers/loads sprites, packs them into the bin, composites the
// atlas image, and writes both the PNG and the sprite map.

use std::fs;
use std::path::Path;

use crate::config::{
    Max_Atlas_Height, Max_Atlas_Width, Max_Sprite_Padding, Min_Atlas_Height, Min_Atlas_Width,
    Min_Auto_Atlas_Size, Min_Sprite_Padding, Pack_Options,
};
use crate::incremental::outputs_are_current;
use crate::packer::{copy_sprite_to_atlas, create_atlas_surface, pack_sprites_skyline};
use crate::sprite_map::{resolve_map_format, save_atlas_sprite_map};
use crate::sprites::{find_and_read_sprites, Sprite, Surface};

// Error is the union of all errors the packing pipeline can return.
#[derive(Debug)]
pub enum Error {
    Message(String),            // general descriptive error
    AtlasTooSmall(Vec<Sprite>), // some sprites did not fit in the bin
    OutputExists(String),       // --no-overwrite and the output already exists
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::Message(value)
    }
}

// Output_Exists_Error reports that an output file already exists while
// --no-overwrite was requested.
//
// Atlas_Too_Small_Error reports that the requested atlas was too small to
// hold every sprite. It carries the sprite slice so the caller can report
// which sprites failed. Surface storage is released automatically when the
// sprite Vec is dropped.

// pack_sprites_to_atlas is the orchestrator for the whole packing run.
//
// It validates the numeric options, optionally refuses to overwrite existing
// outputs, reads all sprites, packs them with the skyline algorithm, blits
// them into an atlas surface, and finally writes the atlas PNG and the
// sprite map file.
//
// Progress is reported to stderr so stdout stays clean for scripting.
//
// All sprite and image data is owned by local values and released in one shot
// when this function returns; there is no arena to manage.
pub fn pack_sprites_to_atlas(opt: &Pack_Options) -> Result<(), Error> {
    // Validate sprite padding
    if opt.sprite_padding < Min_Sprite_Padding || opt.sprite_padding > Max_Sprite_Padding {
        return Err(Error::Message(format!(
            "invalid sprite padding (min: {}, max: {})",
            Min_Sprite_Padding, Max_Sprite_Padding
        )));
    }

    if !opt.auto_size {
        // Validate explicit atlas dimensions. (Only reached when auto-size is
        // NOT engaged; giving --width/--height in main.rs switches the mode
        // to explicit, so these are the dimensions that will actually be used.)
        if opt.atlas_width < Min_Atlas_Width || opt.atlas_width > Max_Atlas_Width {
            return Err(Error::Message(format!(
                "invalid atlas width (min: {}, max: {})",
                Min_Atlas_Width, Max_Atlas_Width
            )));
        }
        if opt.atlas_height < Min_Atlas_Height || opt.atlas_height > Max_Atlas_Height {
            return Err(Error::Message(format!(
                "invalid atlas height (min: {}, max: {})",
                Min_Atlas_Height, Max_Atlas_Height
            )));
        }
    }

    // Skip the build when the outputs are already current (incremental)
    //
    // Config (atlas size, padding, map format, ...) is assumed to not change
    // between runs; only source modifications are considered. Both the atlas
    // PNG and the sprite map must exist and be newer than every source sprite,
    // otherwise the atlas is regenerated. This runs before the --no-overwrite
    // check: an up-to-date set of outputs is a success, not an overwrite.
    if opt.incremental {
        let current = outputs_are_current(&opt.dir, &opt.atlas_path, &opt.sprite_map_path)?;
        if current {
            eprintln!(
                "{} and {} are up to date",
                opt.atlas_path, opt.sprite_map_path
            );
            return Ok(());
        }
        eprintln!("sources changed, rebuilding");
    }

    // Refuse to clobber existing outputs when requested
    if opt.no_overwrite {
        if Path::new(&opt.atlas_path).exists() {
            return Err(Error::OutputExists(opt.atlas_path.clone()));
        }
        if Path::new(&opt.sprite_map_path).exists() {
            return Err(Error::OutputExists(opt.sprite_map_path.clone()));
        }
    }

    // Load all sprites
    let mut sprites = find_and_read_sprites(&opt.dir, ".png")?;
    eprintln!("Found {} sprites", sprites.len());

    // Resolve the atlas dimensions (explicit or auto-sized)
    let (mut width, mut height) = (opt.atlas_width, opt.atlas_height);
    if opt.auto_size {
        // Probe increasing square power-of-two sizes until everything fits.
        // Each probe overwrites the previous offsets; the last probe leaves the
        // final layout in the sprites. On failure the last probe used the
        // largest permitted size, so the sprites reflect which ones failed.
        let mut size = Min_Auto_Atlas_Size;
        loop {
            let packed = pack_sprites_skyline(
                &mut sprites,
                size,
                size,
                opt.sprite_padding,
                opt.heuristic,
                opt.sort,
            );
            if packed == sprites.len() {
                break;
            }
            if size >= Max_Atlas_Width {
                // Reached the largest allowed size without fitting everything.
                return Err(Error::AtlasTooSmall(sprites));
            }
            size *= 2;
        }
        width = size;
        height = size;
    } else {
        let packed = pack_sprites_skyline(
            &mut sprites,
            width,
            height,
            opt.sprite_padding,
            opt.heuristic,
            opt.sort,
        );
        if packed != sprites.len() {
            // Not everything fit. The caller reports the failures.
            return Err(Error::AtlasTooSmall(sprites));
        }
    }

    eprintln!(
        "Packed {} sprites into {}x{} atlas",
        sprites.len(),
        width,
        height
    );

    // Resolve any .Auto map format from the output extension up front, while
    // nothing has been written yet. This way an unresolvable extension fails
    // cleanly instead of truncating a pre-existing map file or leaving a fresh
    // atlas PNG with no map behind.
    let resolved_format = resolve_map_format(opt.map_format, &opt.sprite_map_path)?;

    // Composite the atlas image
    let mut atlas = create_atlas_surface(width, height)?;

    for s in &sprites {
        copy_sprite_to_atlas(&s.surface, &mut atlas, s.offset.0, s.offset.1);
    }

    // Write outputs
    save_texture_atlas(&opt.atlas_path, &atlas)?;

    save_atlas_sprite_map(
        resolved_format,
        &opt.sprite_map_path,
        &opt.map_enum_prefix,
        &opt.map_array_name,
        &opt.map_array_type,
        &sprites,
        &opt.spr_root,
        opt.spr_scale_mode,
    )?;
    eprintln!("Saved {} and {}", opt.atlas_path, opt.sprite_map_path);
    Ok(())
}

// save_texture_atlas encodes the atlas surface to a PNG and writes it to
// atlas_path, creating any missing parent directories.
pub fn save_texture_atlas(atlas_path: &str, atlas: &Surface) -> Result<(), Error> {
    // Create parent directories (ignore "already exists").
    if let Some(dir) = Path::new(atlas_path).parent() {
        if !dir.as_os_str().is_empty() && dir != Path::new(".") {
            if let Err(merr) = fs::create_dir_all(dir) {
                return Err(Error::Message(format!(
                    "cannot create directory for atlas: {merr}"
                )));
            }
        }
    }

    image::save_buffer_with_format(
        atlas_path,
        &atlas.pixels,
        atlas.width,
        atlas.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| Error::Message(format!("error writing texture atlas file: {e}")))?;
    Ok(())
}
