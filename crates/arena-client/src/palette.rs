//! Bevy adapters for the closed palette (docs/palette.json). Presentation code
//! takes every colour from here; spell effects are the only exemption.
use bevy::prelude::*;

pub use arena_palette::*;

/// The swatch as a Bevy sRGB colour.
pub const fn color(swatch: Swatch) -> Color {
    let [r, g, b] = swatch.srgb();
    Color::srgb(r, g, b)
}

/// The swatch's sRGB channels, for procedural painters that blend in sRGB.
pub const fn vec3(swatch: Swatch) -> Vec3 {
    let [r, g, b] = swatch.srgb();
    Vec3::new(r, g, b)
}

#[cfg(test)]
mod tests {
    /// Files whose non-test code may use colours outside the palette.
    const EXEMPT: [&str; 1] = [
        // Spell effects are meant to stand out from the palette.
        "lightning.rs",
    ];
    /// Constructors that build a colour from literal channel values.
    const LITERALS: [&str; 11] = [
        "Color::srgb(",
        "Color::srgba(",
        "Color::linear_rgb(",
        "Color::linear_rgba(",
        "Color::hsl(",
        "Color::hsla(",
        "Color::oklch(",
        "Srgba::new(",
        "Srgba::rgb(",
        "LinearRgba::new(",
        "LinearRgba::rgb(",
    ];
    /// Named colours that are not identity multipliers.
    const NAMED: [&str; 4] = ["Color::BLACK", "Srgba::BLACK", "palettes::", "css::"];

    fn production_code(source: &str) -> &str {
        source
            .find("#[cfg(test)]\nmod tests")
            .map_or(source, |end| &source[..end])
    }

    #[test]
    fn presentation_code_only_uses_palette_colours() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut offences = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.extension().is_none_or(|ext| ext != "rs")
                || EXEMPT.contains(&name.as_str())
                || name == "palette.rs"
            {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for (line, text) in production_code(&source).lines().enumerate() {
                let literal = LITERALS.iter().any(|call| {
                    text.match_indices(call).any(|(at, _)| {
                        let rest = text[at + call.len()..].trim_start();
                        rest.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
                    })
                });
                if literal || NAMED.iter().any(|named| text.contains(named)) {
                    offences.push(format!("{name}:{}: {}", line + 1, text.trim()));
                }
            }
        }
        assert!(
            offences.is_empty(),
            "use palette swatches (docs/palette.json) instead of literal colours:\n{}",
            offences.join("\n")
        );
    }
}
