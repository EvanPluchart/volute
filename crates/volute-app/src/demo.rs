//! Looks for `--mock`: the demo apps get proper names and simple drawn icons, so screenshots
//! show what a real mixer looks like without reading any executable. Each icon is a coloured
//! tile with a white glyph, drawn here with supersampling; no brand's artwork is used.

use std::path::Path;

use volute_shell::Icon;

use crate::mixer::Looks;

pub struct DemoLooks;

impl Looks for DemoLooks {
    fn look(&mut self, exe: &Path, size: u32) -> (Option<String>, Option<Icon>) {
        let stem = exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (name, tile, glyph): (&str, [u8; 3], Glyph) = match stem.as_str() {
            "discord" => ("Discord", [0x4F, 0x6B, 0xF5], Glyph::Chat),
            "firefox" => ("Firefox", [0xF2, 0x6B, 0x2E], Glyph::Globe),
            "spotify" => ("Spotify", [0x1E, 0xA8, 0x62], Glyph::Note),
            "hollow_knight" => ("Hollow Knight", [0x3A, 0x3F, 0x58], Glyph::Pad),
            _ => return (None, None),
        };
        (Some(name.to_owned()), Some(draw(size.max(16), tile, glyph)))
    }
}

#[derive(Clone, Copy)]
enum Glyph {
    Chat,
    Globe,
    Note,
    Pad,
}

/// Samples per pixel on each axis.
const SUPER: u32 = 4;

/// What covers the point (`x`, `y`) of a 48-unit square: 0 nothing, 1 the tile, 2 the glyph.
fn paint(glyph: Glyph, x: f32, y: f32) -> u8 {
    if !rounded(x, y, 1.0, 1.0, 47.0, 47.0, 11.0) {
        return 0;
    }
    let on = match glyph {
        Glyph::Chat => {
            rounded(x, y, 11.0, 13.0, 37.0, 32.0, 7.0)
                // The tail, a triangle under the bubble's left side.
                || ((31.0..=38.0).contains(&y) && (15.0..=23.0 - (y - 31.0)).contains(&x))
        }
        Glyph::Globe => {
            let ring = |rx: f32, ry: f32| {
                let d = ((x - 24.0) / rx).powi(2) + ((y - 24.0) / ry).powi(2);
                let inner = ((x - 24.0) / (rx - 2.6)).powi(2) + ((y - 24.0) / (ry - 2.6)).powi(2);
                d <= 1.0 && inner >= 1.0
            };
            let inside = ((x - 24.0).powi(2) + (y - 24.0).powi(2)) <= 13.0f32.powi(2);
            ring(13.0, 13.0) || (inside && (ring(6.0, 13.0) || (y - 24.0).abs() <= 1.3))
        }
        Glyph::Note => {
            let head = |cx: f32, cy: f32| ((x - cx) / 5.2).powi(2) + ((y - cy) / 4.2).powi(2) <= 1.0;
            head(16.5, 32.0)
                || head(30.5, 29.0)
                || ((19.0..=22.0).contains(&x) && (14.0..=32.0).contains(&y))
                || ((33.0..=36.0).contains(&x) && (11.0..=29.0).contains(&y))
                // The beam, rising to the right.
                || ((19.0..=36.0).contains(&x) && {
                    let top = 14.0 - (x - 19.0) * 3.0 / 17.0;
                    (top..=top + 4.5).contains(&y)
                })
        }
        Glyph::Pad => {
            let body = rounded(x, y, 9.0, 16.0, 39.0, 33.0, 8.5);
            let cross = ((x - 17.0).abs() <= 1.4 && (y - 24.5).abs() <= 4.5)
                || ((y - 24.5).abs() <= 1.4 && (x - 17.0).abs() <= 4.5);
            let button = |cx: f32, cy: f32| (x - cx).powi(2) + (y - cy).powi(2) <= 2.3f32.powi(2);
            body && !cross && !button(29.5, 22.5) && !button(33.0, 26.5)
        }
    };
    if on { 2 } else { 1 }
}

/// Inside the rectangle (`x0`, `y0`)–(`x1`, `y1`) with corners of radius `r`.
fn rounded(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// The icon at `size` pixels: the tile in `tile`, lighter at the top, the glyph in white.
fn draw(size: u32, tile: [u8; 3], glyph: Glyph) -> Icon {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let scale = 48.0 / (size * SUPER) as f32;
    for py in 0..size {
        for px in 0..size {
            let (mut tile_n, mut glyph_n) = (0u32, 0u32);
            for sy in 0..SUPER {
                for sx in 0..SUPER {
                    let x = ((px * SUPER + sx) as f32 + 0.5) * scale;
                    let y = ((py * SUPER + sy) as f32 + 0.5) * scale;
                    match paint(glyph, x, y) {
                        1 => tile_n += 1,
                        2 => glyph_n += 1,
                        _ => {}
                    }
                }
            }
            let total = SUPER * SUPER;
            let covered = tile_n + glyph_n;
            if covered == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // A soft light from above: up to 12 % lighter at the top.
            let lift = 0.12 * (1.0 - py as f32 / size as f32);
            let base = tile.map(|c| c as f32 + (255.0 - c as f32) * lift);
            let mix = glyph_n as f32 / covered as f32;
            let colour = base.map(|c| (c * (1.0 - mix) + 255.0 * mix).round() as u8);
            let alpha = (covered * 255 / total) as u8;
            rgba.extend_from_slice(&[colour[0], colour[1], colour[2], alpha]);
        }
    }
    Icon { width: size, height: size, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_apps_get_a_name_and_an_icon() {
        let (name, icon) = DemoLooks.look(Path::new(r"C:\Program Files\Mozilla Firefox\firefox.exe"), 48);
        assert_eq!(name.as_deref(), Some("Firefox"));
        let icon = icon.expect("an icon");
        assert_eq!((icon.width, icon.height, icon.rgba.len()), (48, 48, 48 * 48 * 4));
        // Transparent corners, an opaque centre.
        assert_eq!(icon.rgba[3], 0);
        let centre = ((24 * 48 + 24) * 4 + 3) as usize;
        assert_eq!(icon.rgba[centre], 255);
        assert_eq!(DemoLooks.look(Path::new(r"C:\other.exe"), 48), (None, None));
    }
}
