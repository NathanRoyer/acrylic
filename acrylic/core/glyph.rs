use super::visual::{Pixels, RgbaPixelBuffer, PixelBuffer, PixelSource};
use super::rgb::RGBA8;
use crate::{Error, error, Vec, Box};

use ttf_parser::OutlineBuilder;
pub(crate) use ttf_parser::Face as Font;

use wizdraw::push_cubic_bezier_segments;
use wizdraw::fill;
use vek::vec::Vec2;
use vek::bezier::CubicBezier2;
use vek::bezier::QuadraticBezier2;
use core::fmt::{self, Write};

const APPLY_SIDE_BEARING: bool = false;

/// Specifies a font variant
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct FontConfig {
    pub weight: Option<Pixels>,
    pub italic_angle: Option<Pixels>,
    pub underline: Option<Pixels>,
    pub overline: Option<Pixels>,
    pub opacity: Option<Pixels>,
    pub serif_rise: Option<Pixels>,
}

fn failed_glyph(mask: Option<&mut Vec<u8>>, font_size: usize) -> (usize, usize) {
    if let Some(mask) = mask {
        mask.fill(255);
        mask.resize(font_size * font_size, 255);
    }

    (font_size, 0)
}

pub fn get_glyph_mask(
    glyph: char,
    font: &Font,
    _font_config: &FontConfig,
    font_size: usize,
    _next_glyph: Option<char>,
    mask: Option<&mut Vec<u8>>,
) -> (usize, usize) {
    let z = Pixels::ZERO;
    let fs_pixels = Pixels::from_num(font_size);

    let glyph_id = match font.glyph_index(glyph) {
        Some(glyph_id) => glyph_id,
        None => {
            log::error!("Font does not contain glyph {:?}", glyph);
            return failed_glyph(mask, font_size);
        },
    };

    let font_height = Pixels::from_num(font.height());
    let scaler = font_height.checked_div(fs_pixels).unwrap_or(z);

    let h_advance = font.glyph_hor_advance(glyph_id).unwrap_or(fs_pixels.to_num());
    let h_advance_scaled = Pixels::from_num(h_advance).checked_div(scaler).unwrap_or(z);
    let unscaled = Vec2::new(0.0, font.ascender().into());

    let h_bearing = font.glyph_hor_side_bearing(glyph_id).unwrap_or(0);
    let h_bearing_scaled = Pixels::from_num(h_bearing).checked_div(scaler).unwrap_or(z);

    let scaled_rounded = Vec2::new(h_advance_scaled.round().to_num(), font_size);
    if let Some(mask) = mask {
        mask.fill(0);
        mask.resize(scaled_rounded.product(), 0);

        let mut outline = Outline::new(unscaled, scaler.to_num());
        if let None = font.outline_glyph(glyph_id, &mut outline) {
            log::error!("Coudn't outline glyph {:?}", glyph);
            return failed_glyph(Some(mask), font_size);
        }

        let segments = outline.finish();
        fill::<_, 6>(&segments, &mut *mask, scaled_rounded);
    }

    (scaled_rounded.x, h_bearing_scaled.round().to_num())
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

pub fn space_width(font_size: usize) -> usize {
    font_size / 4
}

fn interchar_width(font_size: usize) -> usize {
    font_size / 24
}

pub struct StrTexture<'a> {
    font_size: usize,
    width: usize,
    font: Font<'a>,
    config: FontConfig,
    render_data: Option<(Vec<u8>, Vec<u8>, RGBA8)>,
}

impl<'a> StrTexture<'a> {
    pub fn new(font_bytes: &'a [u8], font_size: usize, render: Option<RGBA8>) -> Result<Self, Error> {
        Ok(Self {
            font_size,
            width: 0,
            font: Font::parse(font_bytes, 0).map_err(|e| error!("StrTexture: could not parse font: {}", e))?,
            config: FontConfig::default(),
            render_data: match render {
                Some(color) => Some((Vec::new(), Vec::new(), color)),
                None => None,
            },
        })
    }

    pub fn write<T: fmt::Display + ?Sized>(&mut self, text: &T) {
        core::write!(self, "{}", text).unwrap();
    }

    pub fn width(self) -> usize {
        self.width
    }

    pub fn texture(self) -> PixelSource {
        if let Some((_mask, pixels, _color)) = self.render_data {
            let pixel_buffer = RgbaPixelBuffer::new(pixels.into_boxed_slice(), self.width, self.font_size);
            PixelSource::Texture(Box::new(pixel_buffer))
        } else {
            panic!("StrTexture: No render color → no texture");
        }
    }
}

impl<'a> fmt::Write for StrTexture<'a> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let old_width = self.width;

        for glyph in text.chars() {
            if glyph == ' ' {
                self.width += space_width(self.font_size);
                continue;
            }

            let (advance, side_bearing) = get_glyph_mask(glyph, &self.font, &self.config, self.font_size, None, None);

            if APPLY_SIDE_BEARING && self.width > side_bearing {
                self.width -= side_bearing;
                self.width += interchar_width(self.font_size);
            }

            self.width += advance;
        }

        if let Some((mask, pixels, color)) = &mut self.render_data {
            let old_line_len = old_width * 4;
            let new_line_len = self.width * 4;
            pixels.resize(self.font_size * new_line_len, 0);

            for y in (0..self.font_size).rev() {
                let src_offset = y * old_line_len;
                let limit = src_offset + old_line_len;
                let src_range = src_offset..limit;
                let dst_offset = y * new_line_len;
                pixels.copy_within(src_range, dst_offset);
            }

            let mut cursor = old_width;
            for glyph in text.chars() {
                if glyph == ' ' {
                    let advance = space_width(self.font_size);
                    let mut px_offset = cursor * 4;
                    for _ in 0..self.font_size {
                        pixels[px_offset..px_offset + (advance * 4)].fill(0);
                        px_offset += new_line_len;
                    }
                    cursor += advance;
                    continue;
                }

                let (advance, side_bearing) = get_glyph_mask(glyph, &self.font, &self.config, self.font_size, None, Some(mask));

                if APPLY_SIDE_BEARING && cursor > side_bearing {
                    cursor -= side_bearing;
                    cursor += interchar_width(self.font_size);
                }

                let mut px_offset = cursor * 4;
                let mut mask_offset = 0;
                for _ in 0..self.font_size {
                    pixels[px_offset..px_offset + (advance * 4)].fill(0);
                    for x in 0..advance {
                        let opacity = mask[mask_offset + x];
                        let alpha = match color.a {
                            255 => opacity,
                            v => (((v as u32) * (opacity as u32)) / 255) as u8,
                        };

                        let pixel_x = x * 4;
                        if opacity > 0 {
                            pixels[px_offset + pixel_x + 0] = color.r;
                            pixels[px_offset + pixel_x + 1] = color.g;
                            pixels[px_offset + pixel_x + 2] = color.b;
                            pixels[px_offset + pixel_x + 3] = alpha;
                        }
                    }

                    px_offset += new_line_len;
                    mask_offset += advance;
                }

                cursor += advance;
            }
        }

        Ok(())
    }
}
