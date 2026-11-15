//! Software pixel drawing on a VIEW_W x VIEW_H RGBA8 buffer (straight alpha).
//! Everything is in view pixels; callers convert world -> view via the camera.

use shared::{VIEW_H, VIEW_W};

pub type Rgba = [u8; 4];

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    [r, g, b, a]
}

pub fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Multiply rgb by `k` (0..=2), keeping alpha.
pub fn shade(c: Rgba, k: f32) -> Rgba {
    let m = |x: u8| (x as f32 * k).clamp(0.0, 255.0) as u8;
    [m(c[0]), m(c[1]), m(c[2]), c[3]]
}

pub fn with_alpha(c: Rgba, a: u8) -> Rgba {
    [c[0], c[1], c[2], a]
}

pub struct Canvas<'a> {
    pub px: &'a mut [u8],
    pub w: i32,
    pub h: i32,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u8]) -> Self {
        debug_assert_eq!(px.len(), shared::LAYER_BYTES);
        Canvas { px, w: VIEW_W as i32, h: VIEW_H as i32 }
    }

    pub fn clear(&mut self, c: Rgba) {
        for p in self.px.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
    }

    #[inline]
    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            None
        } else {
            Some(((y * self.w + x) * 4) as usize)
        }
    }

    /// Overwrite a pixel (no blending).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Rgba) {
        if let Some(i) = self.idx(x, y) {
            self.px[i..i + 4].copy_from_slice(&c);
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Rgba {
        match self.idx(x, y) {
            Some(i) => [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]],
            None => [0, 0, 0, 0],
        }
    }

    /// Porter-Duff "over" with straight alpha. Works on opaque and transparent layers.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgba) {
        if c[3] == 0 {
            return;
        }
        if c[3] == 255 {
            return self.put(x, y, c);
        }
        let Some(i) = self.idx(x, y) else { return };
        let sa = c[3] as u32;
        let da = self.px[i + 3] as u32;
        let inv = 255 - sa;
        let oa = sa * 255 + da * inv; // scaled by 255
        if oa == 0 {
            return;
        }
        for k in 0..3 {
            let s = c[k] as u32 * sa * 255;
            let d = self.px[i + k] as u32 * da * inv;
            self.px[i + k] = ((s + d) / oa) as u8;
        }
        self.px[i + 3] = (oa / 255) as u8;
    }

    /// Additive glow (for light sources, sparkles). `a` scales the contribution.
    #[inline]
    pub fn add(&mut self, x: i32, y: i32, c: Rgba) {
        let Some(i) = self.idx(x, y) else { return };
        let a = c[3] as u32;
        for k in 0..3 {
            self.px[i + k] = (self.px[i + k] as u32 + c[k] as u32 * a / 255).min(255) as u8;
        }
        self.px[i + 3] = (self.px[i + 3] as u32 + a).min(255) as u8;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + w).min(self.w), (y + h).min(self.h));
        if c[3] == 255 {
            for yy in y0..y1 {
                let start = ((yy * self.w + x0) * 4) as usize;
                let end = ((yy * self.w + x1) * 4) as usize;
                if start < end {
                    for p in self.px[start..end].chunks_exact_mut(4) {
                        p.copy_from_slice(&c);
                    }
                }
            }
        } else {
            for yy in y0..y1 {
                for xx in x0..x1 {
                    self.blend(xx, yy, c);
                }
            }
        }
    }

    pub fn rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        self.fill_rect(x, y, w, 1, c);
        self.fill_rect(x, y + h - 1, w, 1, c);
        self.fill_rect(x, y + 1, 1, h - 2, c);
        self.fill_rect(x + w - 1, y + 1, 1, h - 2, c);
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.blend(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Soft radial glow, additive. Good for torches, sparks, screen light.
    pub fn glow(&mut self, cx: i32, cy: i32, radius: i32, c: Rgba) {
        let r2 = (radius * radius).max(1) as f32;
        for y in -radius..=radius {
            for x in -radius..=radius {
                let d = (x * x + y * y) as f32 / r2;
                if d < 1.0 {
                    let f = (1.0 - d) * (1.0 - d);
                    self.add(cx + x, cy + y, with_alpha(c, (c[3] as f32 * f) as u8));
                }
            }
        }
    }

    pub fn sprite(&mut self, s: &Sprite, x: i32, y: i32, flip_x: bool) {
        for sy in 0..s.h {
            for sx in 0..s.w {
                let src = if flip_x { s.w - 1 - sx } else { sx };
                let c = s.px[(sy * s.w + src) as usize];
                self.blend(x + sx, y + sy, c);
            }
        }
    }

    /// Sprite tinted by multiplying with `k` (e.g. darkness or hit flash).
    pub fn sprite_shaded(&mut self, s: &Sprite, x: i32, y: i32, flip_x: bool, k: f32) {
        for sy in 0..s.h {
            for sx in 0..s.w {
                let src = if flip_x { s.w - 1 - sx } else { sx };
                let c = s.px[(sy * s.w + src) as usize];
                self.blend(x + sx, y + sy, shade(c, k));
            }
        }
    }

    /// Sprite drawn as a flat colour silhouette (hit flash, shadows).
    pub fn sprite_flat(&mut self, s: &Sprite, x: i32, y: i32, flip_x: bool, c: Rgba) {
        for sy in 0..s.h {
            for sx in 0..s.w {
                let src = if flip_x { s.w - 1 - sx } else { sx };
                let a = s.px[(sy * s.w + src) as usize][3];
                if a > 0 {
                    self.blend(x + sx, y + sy, with_alpha(c, ((a as u32 * c[3] as u32) / 255) as u8));
                }
            }
        }
    }

    /// Pixel text, 3x5 font with 1px spacing, `scale` >= 1. Returns width drawn.
    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgba) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            let g = glyph(ch);
            for row in 0..5 {
                for col in 0..3 {
                    if g[row * 3 + col] == b'#' {
                        self.fill_rect(cx + col as i32 * scale, y + row as i32 * scale, scale, scale, c);
                    }
                }
            }
            cx += 4 * scale;
        }
        cx - x
    }

    /// Text with a 1-scale-pixel dark drop shadow, which reads well on any background.
    pub fn text_shadow(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgba) -> i32 {
        self.text(x + scale, y + scale, s, scale, rgba(10, 10, 20, (c[3] as u32 * 200 / 255) as u8));
        self.text(x, y, s, scale, c)
    }

    /// Centered text with shadow.
    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, scale: i32, c: Rgba) {
        let w = text_width(s, scale);
        self.text_shadow(cx - w / 2, y, s, scale, c);
    }
}

pub fn text_width(s: &str, scale: i32) -> i32 {
    (s.chars().count() as i32 * 4 - 1).max(0) * scale
}

/// RGBA sprite. Build from ASCII art with a palette.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub w: i32,
    pub h: i32,
    pub px: Vec<Rgba>,
}

impl Sprite {
    pub fn new(w: i32, h: i32) -> Self {
        Sprite { w, h, px: vec![[0, 0, 0, 0]; (w * h) as usize] }
    }

    /// Rows of equal length; '.' or ' ' is transparent; other chars look up `palette`.
    pub fn from_ascii(rows: &[&str], palette: &[(char, Rgba)]) -> Self {
        let h = rows.len() as i32;
        let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
        let mut s = Sprite::new(w, h);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if let Some((_, c)) = palette.iter().find(|(k, _)| *k == ch) {
                    s.px[y * w as usize + x] = *c;
                }
            }
        }
        s
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize]
        } else {
            [0, 0, 0, 0]
        }
    }
}

/// 3x5 glyphs, row-major, '#' = on. Lowercase maps to uppercase; unknown -> '?'.
fn glyph(ch: char) -> &'static [u8; 15] {
    let ch = ch.to_ascii_uppercase();
    GLYPHS
        .iter()
        .find(|(k, _)| *k == ch)
        .map(|(_, g)| *g)
        .unwrap_or(GLYPHS[GLYPHS.len() - 1].1)
}

static GLYPHS: &[(char, &[u8; 15])] = &[
    ('A', b".#.#.#####.##.#"),
    ('B', b"##.#.###.#.###."),
    ('C', b".###..#..#...##"),
    ('D', b"##.#.##.##.###."),
    ('E', b"####..####..###"),
    ('F', b"####..####..#.."),
    ('G', b".###..#.##.#.##"),
    ('H', b"#.##.#####.##.#"),
    ('I', b"###.#..#..#.###"),
    ('J', b"..#..#..##.#.#."),
    ('K', b"#.##.###.#.##.#"),
    ('L', b"#..#..#..#..###"),
    ('M', b"#.#####.##.##.#"),
    ('N', b"##.#.##.##.##.#"),
    ('O', b".#.#.##.##.#.#."),
    ('P', b"##.#.###.#..#.."),
    ('Q', b".#.#.##.##.#..#"),
    ('R', b"##.#.###.#.##.#"),
    ('S', b".###...#...###."),
    ('T', b"###.#..#..#..#."),
    ('U', b"#.##.##.##.####"),
    ('V', b"#.##.##.##.#.#."),
    ('W', b"#.##.##.#####.#"),
    ('X', b"#.##.#.#.#.##.#"),
    ('Y', b"#.##.#.#..#..#."),
    ('Z', b"###..#.#.#..###"),
    ('0', b"####.##.##.####"),
    ('1', b".#.##..#..#.###"),
    ('2', b"##...#.#.#..###"),
    ('3', b"##...#.#...###."),
    ('4', b"#.##.####..#..#"),
    ('5', b"####..##...###."),
    ('6', b".###..####.####"),
    ('7', b"###..#..#.#..#."),
    ('8', b"####.#####.####"),
    ('9', b"####.####..###."),
    (' ', b"..............."),
    ('.', b".............#."),
    (',', b"..........#.#.."),
    ('!', b".#..#..#.....#."),
    (':', b"....#.....#...."),
    ('-', b"......###......"),
    ('+', b"....#.###.#...."),
    ('/', b"..#..#.#.#..#.."),
    ('%', b"#.#..#.#.#..#.#"),
    ('\'', b".#..#.........."),
    ('(', b"..#.#..#..#...#"),
    (')', b"#...#..#..#.#.."),
    ('>', b"#...#...#.#.#.."),
    ('<', b"..#.#.#...#...#"),
    ('=', b"...###...###..."),
    ('_', b"............###"),
    ('|', b".#..#..#..#..#."),
    ('[', b"##.#..#..#..##."),
    (']', b".##..#..#..#.##"),
    ('@', b".#.#.####...##."),
    ('$', b".####..#..####."),
    ('~', b"...##..##......"),
    ('?', b"##...#.#.....#."),
];
