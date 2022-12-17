//! FontConfig, Outline, get_glyph, GlyphCache

use crate::Size;
use crate::round;

use ttf_parser::OutlineBuilder;
pub(crate) use ttf_parser::Face as Font;

use vek::vec::Vec2;
use vek::bezier::CubicBezier2;
use vek::bezier::QuadraticBezier2;

use wizdraw::push_cubic_bezier_segments;
use wizdraw::fill;

use hashbrown::hash_map::HashMap;

use log::error;

use alloc::vec;
use alloc::vec::Vec;
use alloc::sync::Arc;

/// 1/100 of a value
pub type Hundredth = usize;

/// An index into the app's fonts
pub type FontIndex = usize;

/// Specifies a font variant
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct FontConfig {
    pub weight: Option<Hundredth>,
    pub italic_angle: Option<Hundredth>,
    pub underline: Option<Hundredth>,
    pub overline: Option<Hundredth>,
    pub opacity: Option<Hundredth>,
    pub serif_rise: Option<Hundredth>,
}

/// A cache of rendered glyphs
///
/// Key is `(font_index, font_size, config, character)`
/// Value is `(size, pixel mask)`.
pub type GlyphCache = HashMap<(usize, usize, FontConfig, char), Arc<(Size, isize, Vec<u8>)>>;

/// Used internally to obtain a rendered glyph
/// from the font, which is then kept in cache.
///
/// Returns a placeholder if the glyph cannot be
/// obtained.
pub fn get_glyph_mask(
    glyph: char,
    font: &Font,
    font_config: FontConfig,
    font_size: usize,
    next_glyph: Option<char>,
) -> (Size, isize, Vec<u8>) {
    match try_get_glyph_mask(glyph, font, font_config, font_size, next_glyph) {
        Ok(mask) => mask,
        Err(error) => {
            error!("try_get_glyph_mask: {}", error);

            // return an opaque square
            (Size::new(font_size, font_size), 0, vec![255; font_size * font_size])
        },
    }
}

/// Used internally to obtain a rendered glyph
/// from the font, which is then kept in cache.
pub fn try_get_glyph_mask(
    glyph: char,
    font: &Font,
    _font_config: FontConfig,
    font_size: usize,
    _next_glyph: Option<char>,
) -> Result<(Size, isize, Vec<u8>), &'static str> {
    let glyph_id = font.glyph_index(glyph).ok_or("can't find glyph in font")?;

    let font_height = font.height();
    let scaler = (font_height as f32) / (font_size as f32);

    let h_advance = font.glyph_hor_advance(glyph_id)
                        .ok_or("bad glyph: no horizontal advance")?;

    let h_advance = (h_advance as f32) / scaler;

    let h_bearing = font.glyph_hor_side_bearing(glyph_id)
                        .ok_or("bad glyph: no horizontal bearing")?;

    let h_bearing = ((h_bearing as f32) / scaler).trunc() as isize;

    let size_vec2 = Vec2::new(h_advance, font.ascender() as f32);
    let h_advance = round!(h_advance, f32, usize);
    let size = Size::new(h_advance, font_size);

    let mut outline = Outline::new(size_vec2, scaler);
    font.outline_glyph(glyph_id, &mut outline)
        .ok_or("Couldn't outline glyph")?;
    let segments = outline.finish();

    let mut mask = vec![0; size.w * size.h];
    let size_vec2 = Vec2::new(size.w, size.h);
    fill::<_, 6>(&segments, &mut mask, size_vec2);

    Ok((size, h_bearing, mask))
}

pub struct Outline {
    points: Vec<Vec2<f32>>,
    last_point: Vec2<f32>,
    base: Vec2<f32>,
    scaler: f32,
}

impl Outline {
    pub fn new(base: Vec2<f32>, scaler: f32) -> Self {
        Self {
            points: Vec::new(),
            last_point: Vec2::zero(),
            base,
            scaler,
        }
    }

    pub fn adjusted(&self, x: f32, y: f32) -> Vec2<f32> {
        Vec2 {
            x: (x - self.base.x) / self.scaler,
            y: (self.base.y - y) / self.scaler,
        }
    }

    pub fn finish(self) -> Vec<Vec2<f32>> {
        self.points
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        // assuming this is the first push
        self.last_point = self.adjusted(x, y);
        self.points.push(self.last_point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.last_point = self.adjusted(x, y);
        self.points.push(self.last_point);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ctrl = self.adjusted(x1, y1);
        let end = self.adjusted(x, y);
        let curve = QuadraticBezier2 {
            start: self.last_point,
            ctrl,
            end,
        };
        let curve = CubicBezier2::from(curve);
        push_cubic_bezier_segments::<_, 6>(&curve, 0.2, &mut self.points);
        self.last_point = end;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ctrl0 = self.adjusted(x1, y1);
        let ctrl1 = self.adjusted(x2, y2);
        let end = self.adjusted(x, y);
        let curve = CubicBezier2 {
            start: self.last_point,
            ctrl0,
            ctrl1,
            end,
        };
        push_cubic_bezier_segments::<_, 6>(&curve, 0.2, &mut self.points);
        self.last_point = end;
    }

    fn close(&mut self) {
        if self.points.first().is_some() {
            self.points.push(self.points[0]);
        }
    }
}
