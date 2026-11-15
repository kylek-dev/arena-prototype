//! PNG stdin -> bounded PNG stdout. Used only by offline character preparation.
use std::io::{Cursor, Read, Write};

use arena_textures::character::{Kind, resize};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [kind, maximum] = args.as_slice() else {
        return Err("usage: character-textures <color|normal|data> <maximum-edge>".into());
    };
    let kind = match kind.as_str() {
        "color" => Kind::Color,
        "normal" => Kind::Normal,
        "data" => Kind::Data,
        _ => return Err("unknown texture kind".into()),
    };
    let maximum = maximum.parse::<u32>().map_err(|error| error.to_string())?;
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    let source = image::load_from_memory_with_format(&input, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let prepared = resize(&source, maximum, kind)?;
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(prepared)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    std::io::stdout()
        .write_all(output.get_ref())
        .map_err(|error| error.to_string())
}
