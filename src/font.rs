extern crate alloc;

use alloc::vec::Vec;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::ui::UI;

pub struct FontRenderer {
    bytes: Vec<u8>,
}

impl FontRenderer {
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        if Face::parse(&bytes, 0).is_ok() {
            Some(Self { bytes })
        } else {
            None
        }
    }

    pub fn draw_text(
        &self,
        ui: &mut UI,
        x: usize,
        y: usize,
        px_size: usize,
        text: &str,
        color: u32,
    ) -> bool {
        let Ok(face) = Face::parse(&self.bytes, 0) else {
            return false;
        };
        let units = face.units_per_em() as f32;
        let scale = px_size as f32 / units;
        let baseline = y as f32 + px_size as f32;
        let mut pen_x = x as f32;

        for ch in text.chars() {
            if ch == ' ' {
                pen_x += px_size as f32 * 0.35;
                continue;
            }
            let Some(id) = face.glyph_index(ch) else {
                pen_x += px_size as f32 * 0.5;
                continue;
            };
            self.draw_glyph_outline(ui, &face, id, pen_x, baseline, scale, color);
            let advance = face
                .glyph_hor_advance(id)
                .unwrap_or(face.units_per_em() / 2) as f32;
            pen_x += advance * scale;
        }
        true
    }

    fn draw_glyph_outline(
        &self,
        ui: &mut UI,
        face: &Face<'_>,
        id: GlyphId,
        x: f32,
        baseline: f32,
        scale: f32,
        color: u32,
    ) {
        let mut outline = Outline::new(x, baseline, scale);
        if face.outline_glyph(id, &mut outline).is_none() {
            return;
        }
        fill_segments(ui, &outline.segments, color);
        for segment in &outline.segments {
            ui.draw_line(segment.x0, segment.y0, segment.x1, segment.y1, color);
        }
    }
}

fn fill_segments(ui: &mut UI, segments: &[Segment], color: u32) {
    if segments.is_empty() {
        return;
    }
    let mut min_y = usize::MAX;
    let mut max_y = 0;

    for segment in segments {
        min_y = min_y.min(segment.y0).min(segment.y1);
        max_y = max_y.max(segment.y0).max(segment.y1);
    }

    let mut xs = Vec::<isize>::new();
    for y in min_y..=max_y {
        xs.clear();
        for segment in segments {
            let y0 = segment.y0 as isize;
            let y1 = segment.y1 as isize;
            let scan = y as isize;
            if !((y0 <= scan && y1 > scan) || (y1 <= scan && y0 > scan)) {
                continue;
            }
            let x0 = segment.x0 as isize;
            let x1 = segment.x1 as isize;
            let x = x0 + (scan - y0) * (x1 - x0) / (y1 - y0);
            xs.push(x);
        }
        if xs.is_empty() {
            continue;
        }
        xs.sort_unstable();
        let mut i = 0;
        while i + 1 < xs.len() {
            let x0 = xs[i].max(0) as usize;
            let x1 = xs[i + 1].max(0) as usize;
            if x1 > x0 {
                ui.fill_rect(x0, y, x1 - x0, 1, color);
            }
            i += 2;
        }
    }
}

#[derive(Clone, Copy)]
struct Segment {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

struct Outline {
    x: f32,
    baseline: f32,
    scale: f32,
    current: (f32, f32),
    start: (f32, f32),
    segments: Vec<Segment>,
}

impl Outline {
    fn new(x: f32, baseline: f32, scale: f32) -> Self {
        Self {
            x,
            baseline,
            scale,
            current: (0.0, 0.0),
            start: (0.0, 0.0),
            segments: Vec::new(),
        }
    }

    fn point(&self, x: f32, y: f32) -> (usize, usize) {
        let sx = self.x + x * self.scale;
        let sy = self.baseline - y * self.scale;
        (sx.max(0.0) as usize, sy.max(0.0) as usize)
    }

    fn push_line(&mut self, from: (f32, f32), to: (f32, f32)) {
        let (x0, y0) = self.point(from.0, from.1);
        let (x1, y1) = self.point(to.0, to.1);
        self.segments.push(Segment { x0, y0, x1, y1 });
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        self.start = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let next = (x, y);
        self.push_line(self.current, next);
        self.current = next;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let start = self.current;
        let mut prev = start;
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let mt = 1.0 - t;
            let next = (
                mt * mt * start.0 + 2.0 * mt * t * x1 + t * t * x,
                mt * mt * start.1 + 2.0 * mt * t * y1 + t * t * y,
            );
            self.push_line(prev, next);
            prev = next;
        }
        self.current = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let start = self.current;
        let mut prev = start;
        for i in 1..=10 {
            let t = i as f32 / 10.0;
            let mt = 1.0 - t;
            let next = (
                mt * mt * mt * start.0
                    + 3.0 * mt * mt * t * x1
                    + 3.0 * mt * t * t * x2
                    + t * t * t * x,
                mt * mt * mt * start.1
                    + 3.0 * mt * mt * t * y1
                    + 3.0 * mt * t * t * y2
                    + t * t * t * y,
            );
            self.push_line(prev, next);
            prev = next;
        }
        self.current = (x, y);
    }

    fn close(&mut self) {
        self.push_line(self.current, self.start);
        self.current = self.start;
    }
}
