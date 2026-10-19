use std::sync::LazyLock;

/// The DISARMED glyph signature, and its match against a frame's bottom-left.
///
/// MAX7456 glyphs are a bright core wrapped in a dark outline, so the signature
/// is the set of bright pixels with a dark pixel within two — robust over busy
/// backgrounds where plain NCC is not: measured on PICT0032, true DISARMED
/// frames over trees scored NCC 0.4–0.5, inside the armed distribution, while
/// the signature overlap stays separable (spike/fpv_spike.py).
///
/// The match is optional by design (CLAUDE.md 9.8): it tags a clip's `entry`,
/// it never moves a boundary, and no template at all just means `Unknown`.
#[derive(Debug, Clone)]
pub struct DisarmedTemplate {
    pub width: usize,
    pub height: usize,
    signature: Vec<bool>,
    glyph_pixel_count: usize,
}

/// The template extracted from a real disarmed PICT0032 frame
/// (spike/reference/disarmed_template.png, as raw 153x26 grayscale).
const BUNDLED_PIXELS: &[u8] = include_bytes!("../resources/disarmed_template.gray");

static BUNDLED: LazyLock<Option<DisarmedTemplate>> =
    LazyLock::new(|| DisarmedTemplate::new(BUNDLED_PIXELS, 153, 26));

impl DisarmedTemplate {
    /// Where DISARMED sits on this layout, bottom-left, NTSC 640×480. The
    /// leading D is half off-frame (analog overscan). PIL crop order:
    /// (left, top, right, bottom).
    const BOX: (usize, usize, usize, usize) = (12, 362, 165, 388);
    /// How far around the box the match may slide: the OSD drifts a few pixels
    /// between goggles.
    const SEARCH_PAD: usize = 6;
    const BRIGHT_FLOOR: u8 = 180;
    const DARK_CEILING: u8 = 70;

    /// `None` when the crop clearly holds no glyphs — under 100 signature
    /// pixels is a bad template, not a tolerable one (the reference has 325).
    pub fn new(pixels: &[u8], width: usize, height: usize) -> Option<Self> {
        if pixels.len() != width * height {
            return None;
        }
        let signature = Self::signature(pixels, width, height);
        let glyph_pixel_count = signature.iter().filter(|&&lit| lit).count();
        (glyph_pixel_count >= 100).then_some(Self { width, height, signature, glyph_pixel_count })
    }

    /// The template checked in with the crate.
    pub fn bundled() -> Option<&'static DisarmedTemplate> {
        BUNDLED.as_ref()
    }

    pub fn glyph_pixel_count(&self) -> usize {
        self.glyph_pixel_count
    }

    /// Fraction of the template's glyph pixels reproduced in the frame's
    /// DISARMED box, at the best small shift. 1.0 = every glyph pixel lit at
    /// its position.
    pub fn match_frame(&self, frame: &[u8], frame_width: usize, frame_height: usize) -> f64 {
        let (box_left, box_top, box_right, box_bottom) = Self::BOX;
        let left = box_left.saturating_sub(Self::SEARCH_PAD);
        let top = box_top.saturating_sub(Self::SEARCH_PAD);
        let right = frame_width.min(box_right + Self::SEARCH_PAD);
        let bottom = frame_height.min(box_bottom + Self::SEARCH_PAD);
        if frame.len() != frame_width * frame_height || right < left || bottom < top {
            return 0.0;
        }
        let (region_width, region_height) = (right - left, bottom - top);
        if region_width < self.width || region_height < self.height {
            return 0.0;
        }
        let mut region = Vec::with_capacity(region_width * region_height);
        for y in top..bottom {
            region.extend_from_slice(&frame[y * frame_width + left..y * frame_width + right]);
        }
        self.match_region(&region, region_width, region_height)
    }

    /// The slide-and-count core, separable for the parity tests.
    pub fn match_region(&self, region: &[u8], region_width: usize, region_height: usize) -> f64 {
        let region_signature = Self::signature(region, region_width, region_height);
        let mut best = 0;
        for offset_y in 0..=region_height - self.height {
            for offset_x in 0..=region_width - self.width {
                let mut overlap = 0;
                for y in 0..self.height {
                    let region_row = (offset_y + y) * region_width + offset_x;
                    let template = &self.signature[y * self.width..(y + 1) * self.width];
                    let candidate = &region_signature[region_row..region_row + self.width];
                    overlap += template.iter().zip(candidate).filter(|&(&a, &b)| a && b).count();
                }
                best = best.max(overlap);
            }
        }
        best as f64 / self.glyph_pixel_count.max(1) as f64
    }

    /// Bright pixels with a dark pixel within two. The neighbourhood wraps at
    /// the edges — the reference computes it with `np.roll`, and parity with
    /// the numbers every threshold was measured against outranks edge
    /// exactness on a 2-pixel border.
    fn signature(gray: &[u8], width: usize, height: usize) -> Vec<bool> {
        let (w, h) = (width as isize, height as isize);
        let mut near_dark = vec![false; gray.len()];
        for y in 0..h {
            for x in 0..w {
                if gray[(y * w + x) as usize] >= Self::DARK_CEILING {
                    continue;
                }
                for dy in -2..=2 {
                    let row = (y + dy).rem_euclid(h) * w;
                    for dx in -2..=2 {
                        near_dark[(row + (x + dx).rem_euclid(w)) as usize] = true;
                    }
                }
            }
        }
        for (lit, &pixel) in near_dark.iter_mut().zip(gray) {
            *lit = *lit && pixel > Self::BRIGHT_FLOOR;
        }
        near_dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_bundled_template() {
        let template = DisarmedTemplate::bundled().expect("bundled template");
        assert_eq!((template.width, template.height), (153, 26));
        // The Python loader counts 325 glyph pixels in this exact crop.
        assert_eq!(template.glyph_pixel_count(), 325);
    }

    #[test]
    fn reproduces_itself_completely() {
        // Matched against its own bytes — same wrap-around signature both
        // sides — every glyph pixel must land: Python reports exactly 1.0.
        let template = DisarmedTemplate::bundled().unwrap();
        assert_eq!(template.match_region(BUNDLED_PIXELS, 153, 26), 1.0);
    }
}
