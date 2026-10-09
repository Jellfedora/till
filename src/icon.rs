//! Procedurally drawn app icon: a cream acoustic guitar on an amber rounded tile.

type Rgb = [f32; 3];

const BG_TOP: Rgb = [240.0, 156.0, 64.0];
const BG_BOTTOM: Rgb = [206.0, 104.0, 28.0];
const WOOD: Rgb = [255.0, 244.0, 226.0];
const DARK: Rgb = [58.0, 34.0, 14.0];

fn rounded_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    x >= x0 && x <= x1 && y >= y0 && y <= y1 && (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Color of the guitar layer at (x, y) in guitar space (upright, 0..1), if any.
fn guitar(x: f32, y: f32) -> Option<Rgb> {
    let body = circle(x, y, 0.5, 0.71, 0.19)
        || circle(x, y, 0.5, 0.48, 0.145)
        || rounded_rect(x, y, 0.40, 0.48, 0.60, 0.71, 0.0);
    let neck = rounded_rect(x, y, 0.468, 0.12, 0.532, 0.50, 0.01);
    let head = rounded_rect(x, y, 0.435, 0.02, 0.565, 0.15, 0.03);
    let pegs = [0.05, 0.085, 0.12]
        .iter()
        .any(|&py| circle(x, y, 0.425, py + 0.01, 0.016) || circle(x, y, 0.575, py + 0.01, 0.016));
    if !(body || neck || head || pegs) {
        return None;
    }
    let hole = circle(x, y, 0.5, 0.56, 0.058);
    let bridge = rounded_rect(x, y, 0.42, 0.765, 0.58, 0.79, 0.008);
    let nut = rounded_rect(x, y, 0.462, 0.145, 0.538, 0.16, 0.0);
    let string = [0.484, 0.5, 0.516].iter().any(|&sx| (x - sx).abs() < 0.0035) && y > 0.15 && y < 0.778;
    if hole || bridge || nut || pegs {
        return Some(DARK);
    }
    if string {
        return Some([200.0, 150.0, 90.0]);
    }
    Some(WOOD)
}

fn sample(u: f32, v: f32) -> Option<Rgb> {
    if !rounded_rect(u, v, 0.03, 0.03, 0.97, 0.97, 0.21) {
        return None;
    }
    // Guitar tilted 32° around the tile center.
    let (s, c) = (-32f32).to_radians().sin_cos();
    let (dx, dy) = (u - 0.5, v - 0.5);
    let (gx, gy) = (0.5 + (dx * c - dy * s) / 0.92, 0.5 + (dx * s + dy * c) / 0.92);
    if let Some(col) = guitar(gx, gy) {
        return Some(col);
    }
    // Soft drop shadow under the guitar.
    if guitar(gx - 0.018, gy - 0.025).is_some() {
        let t = v;
        let bg: Rgb = std::array::from_fn(|i| BG_TOP[i] + (BG_BOTTOM[i] - BG_TOP[i]) * t);
        return Some(bg.map(|c| c * 0.82));
    }
    Some(std::array::from_fn(|i| BG_TOP[i] + (BG_BOTTOM[i] - BG_TOP[i]) * v))
}

/// RGBA pixels of a `size`×`size` icon, 4×4 supersampled.
pub fn rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let v = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if let Some(c) = sample(u, v) {
                        acc[0] += c[0];
                        acc[1] += c[1];
                        acc[2] += c[2];
                        acc[3] += 1.0;
                    }
                }
            }
            let n = (SS * SS) as f32;
            if acc[3] > 0.0 {
                out.extend([acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]].map(|c| c.round() as u8));
                out.push((acc[3] / n * 255.0).round() as u8);
            } else {
                out.extend([0, 0, 0, 0]);
            }
        }
    }
    out
}

pub fn icon_data(size: u32) -> eframe::egui::IconData {
    eframe::egui::IconData { rgba: rgba(size), width: size, height: size }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dump_icon() {
        if let Ok(path) = std::env::var("TILL_ICON_DUMP") {
            std::fs::write(path, super::rgba(256)).unwrap();
        }
    }
}
