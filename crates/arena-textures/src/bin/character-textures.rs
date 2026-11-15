//! PNG stdin -> bounded PNG stdout. Used only by offline character preparation.
//! `iris RRGGBB` instead recolours the iris of a centred eye texture.
use std::io::{Cursor, Read, Write};

use arena_textures::character::{Kind, recolor_iris, resize};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [kind, argument] = args.as_slice() else {
        return Err(
            "usage: character-textures <color|normal|data> <maximum-edge> | iris <RRGGBB>".into(),
        );
    };
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    let source = image::load_from_memory_with_format(&input, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let prepared = if kind == "iris" {
        let hex = u32::from_str_radix(argument, 16)
            .ok()
            .filter(|_| argument.len() == 6)
            .ok_or("iris colour must be RRGGBB")?;
        recolor_iris(&source, [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8])?
    } else {
        let kind = match kind.as_str() {
            "color" => Kind::Color,
            "normal" => Kind::Normal,
            "data" => Kind::Data,
            _ => return Err("unknown texture kind".into()),
        };
        let maximum = argument.parse::<u32>().map_err(|error| error.to_string())?;
        resize(&source, maximum, kind)?
    };
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(prepared)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    std::io::stdout()
        .write_all(output.get_ref())
        .map_err(|error| error.to_string())
}
