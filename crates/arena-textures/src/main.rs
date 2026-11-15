//! `cargo run -p arena-textures --release -- <source-dir> <output-dir>`
//! `cargo run -p arena-textures --release -- --painted <output-dir>`
//!
//! Reads the downloaded CC0 maps named in `TEXTURES` and writes small tiling
//! PNGs for the game, then paints the original sky cubemap and pillar granite.
//! The `--painted` form (alias `--sky`) writes only those and needs no
//! downloads. scripts/fetch_textures.py downloads and verifies sources.
use std::path::{Path, PathBuf};

use arena_textures::{Recipe, Rgb, granite, prepare, sky};

/// Texels per sky cube face edge.
const SKY_FACE: u32 = 1024;
/// Texels per edge of the 2 m pillar granite tile (384 per metre).
const GRANITE_SIZE: u32 = 768;

struct Texture {
    output: &'static str,
    color: &'static str,
    occlusion: Option<&'static str>,
    recipe: Recipe,
}

/// Mean sRGB luma of the arena floor stone.
const STONE_MEAN: f32 = 0.72;
/// The bricks' higher contrast raises their linear average at the same luma;
/// this mean matches the floor's linear average, which is what tints multiply.
const BRICKS_MEAN: f32 = 0.708;

const TEXTURES: [Texture; 9] = [
    Texture {
        output: "stone-floor.png",
        color: "Stylized_Stone_Floor_010_basecolor.png",
        occlusion: Some("Stylized_Stone_Floor_010_ambientOcclusion.png"),
        // Arena stone (floor, bricks) is pure value with one shared average,
        // so the same palette tint looks the same on every stone surface and
        // floors, walls and pillars meet without a colour seam.
        recipe: Recipe {
            chroma: 0.0,
            occlusion: 0.4,
            saturated_lift: 0.0,
            // Clean, readable ground: keep the stones, soften the grout lines.
            contrast: 0.7,
            mean: STONE_MEAN,
        },
    },
    Texture {
        output: "stone-bricks.png",
        color: "Stylized_Bricks_005_basecolor.png",
        // Its occlusion map is nearly flat; the painted colour carries the depth.
        occlusion: None,
        recipe: Recipe {
            chroma: 0.0,
            occlusion: 0.0,
            saturated_lift: 0.5,
            contrast: 0.85,
            mean: BRICKS_MEAN,
        },
    },
    Texture {
        output: "stone-wall.png",
        color: "Stylized_Stone_Wall_002_basecolor.png",
        occlusion: Some("Stylized_Stone_Wall_002_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.1,
            occlusion: 0.5,
            saturated_lift: 0.0,
            // Calm the painted grime streaks; keep the carved chevron frieze.
            contrast: 0.75,
            mean: 0.66,
        },
    },
    Texture {
        output: "wood-planks.png",
        color: "Stylized_Wood_Planks_002_basecolor.png",
        occlusion: Some("Stylized_Wood_Planks_002_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.3,
            occlusion: 0.4,
            saturated_lift: 0.0,
            contrast: 0.9,
            mean: 0.6,
        },
    },
    Texture {
        output: "metal-grate.png",
        color: "Stylized_Metal_Plates_001_basecolor.png",
        occlusion: Some("Stylized_Metal_Plates_001_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.05,
            occlusion: 0.5,
            // The saturated lattice reads brighter than its frames once grey.
            saturated_lift: 0.35,
            contrast: 0.95,
            mean: 0.55,
        },
    },
    Texture {
        output: "metal-panels.png",
        color: "Stylized_Metal_Pattern_001_basecolor.png",
        occlusion: Some("Stylized_Metal_Pattern_001_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.0,
            occlusion: 0.5,
            saturated_lift: 0.0,
            contrast: 0.9,
            mean: 0.7,
        },
    },
    // Ground options for map floors keep most of their painted colour; the
    // game shows them untinted.
    Texture {
        output: "ground-sand.png",
        color: "Stylized_Sand_002_basecolor.png",
        occlusion: Some("Stylized_Sand_002_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.75,
            occlusion: 0.3,
            saturated_lift: 0.0,
            contrast: 0.75,
            mean: 0.62,
        },
    },
    Texture {
        output: "ground-snow.png",
        color: "Snow_005_basecolor.png",
        occlusion: Some("Snow_005_ambientOcclusion.png"),
        recipe: Recipe {
            chroma: 0.8,
            occlusion: 0.3,
            saturated_lift: 0.0,
            // The source is very flat; stretch it so drifts read from above.
            contrast: 1.35,
            mean: 0.86,
        },
    },
    Texture {
        output: "ground-grass.png",
        color: "Stylized_Grass_003_basecolor.jpg",
        occlusion: Some("Stylized_Grass_003_ambientOcclusion.jpg"),
        recipe: Recipe {
            chroma: 0.85,
            occlusion: 0.3,
            saturated_lift: 0.0,
            // Calm the blade pattern so it does not shimmer at distance.
            contrast: 0.65,
            mean: 0.42,
        },
    },
];

fn write_rgb(
    output: &Path,
    name: &str,
    width: u32,
    height: u32,
    data: Vec<u8>,
) -> Result<(), String> {
    let path = output.join(name);
    image::RgbImage::from_raw(width, height, data)
        .ok_or(format!("{name}: buffer size mismatch"))?
        .save(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    println!("{} ({width}×{height})", path.display());
    Ok(())
}

/// The original paintings: no downloads needed.
fn write_painted(output: &Path) -> Result<(), String> {
    write_rgb(
        output,
        "sky.png",
        SKY_FACE,
        SKY_FACE * 6,
        sky::render(SKY_FACE),
    )?;
    let size = GRANITE_SIZE;
    write_rgb(
        output,
        "stone-granite.png",
        size,
        size,
        granite::render(size),
    )
}

fn read(path: &Path) -> Result<image::DynamicImage, String> {
    image::open(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn main() -> Result<(), String> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if let [flag, output] = args.as_slice()
        && (flag.as_os_str() == "--painted" || flag.as_os_str() == "--sky")
    {
        std::fs::create_dir_all(output).map_err(|error| error.to_string())?;
        return write_painted(output);
    }
    let [source, output] = args.as_slice() else {
        return Err(
            "usage: arena-textures <source-dir> <output-dir> | --painted <output-dir>".into(),
        );
    };
    std::fs::create_dir_all(output).map_err(|error| error.to_string())?;
    for texture in &TEXTURES {
        let color = read(&source.join(texture.color))?.to_rgb8();
        let (width, height) = color.dimensions();
        let color = Rgb {
            width,
            height,
            pixels: color.pixels().map(|p| p.0).collect(),
        };
        let occlusion = match texture.occlusion {
            Some(name) => {
                let ao = read(&source.join(name))?.to_luma8();
                if ao.dimensions() != (width, height) {
                    return Err(format!("{name}: size differs from its colour map"));
                }
                Some(ao.into_raw())
            }
            None => None,
        };
        let prepared = prepare(&color, occlusion.as_deref(), texture.recipe);
        let path = output.join(texture.output);
        image::RgbImage::from_raw(prepared.width, prepared.height, prepared.pixels.concat())
            .ok_or("prepared buffer size mismatch")?
            .save(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        println!(
            "{} ({}×{})",
            path.display(),
            prepared.width,
            prepared.height
        );
    }
    write_painted(output)
}
