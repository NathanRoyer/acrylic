//! Font Parsing & Glyph Rasterization
//!
//! todo: implement <https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf>

use super::event::{Handlers, DEFAULT_HANDLERS};
use super::visual::{RgbaPixelBuffer, GrayScalePixelBuffer, PixelBuffer, PixelSource};
use super::rgb::RGBA8;
use super::app::{Application, Mutator, MutatorIndex, FONT_MUTATOR_INDEX, Storage};
use super::node::NodeKey;
use crate::{Error, Vec, Box, HashMap, CheapString, cheap_string, Rc};
use core::{fmt::{self, Write}};

use ttf_parser::{Tag, Face, OutlineBuilder};

use wizdraw::{push_cubic_bezier_segments, fill};
use vek::{Vec2, QuadraticBezier2, CubicBezier2};
use rgb::FromSlice;

const APPLY_SIDE_BEARING: bool = false;

type GlyphCache = HashMap<(char, usize), Rc<GrayScalePixelBuffer>>;

const WGHT: Tag = Tag::from_bytes(b"wght");

fn failed_glyph(font_size: usize) -> Rc<GrayScalePixelBuffer> {
    let width = font_size;
    let height = font_size;
    let len = width * height;
    let mut mask = Vec::with_capacity(len);
    mask.resize(len, 255);
    let mask = mask.into_boxed_slice();
    Rc::new(GrayScalePixelBuffer::new(mask, width, height))
}

/// Raw font bytes & glyph cache (a HashMap)
pub struct Font {
    bytes: Box<[u8]>,
    glyph_cache: GlyphCache,
    glyph_cache_weight: usize,
}

/// A short-lived multifunction structure
///
/// It can either render glyphs to a texture, or just compute the width of the text.
pub struct GlyphRenderer<'a> {
    font_face: Face<'a>,
    glyph_cache: &'a mut GlyphCache,
    glyph_cache_weight: &'a mut usize,
    render_data: Option<(Vec<u8>, RGBA8)>,
    font_size: usize,
    width: usize,
}

impl Font {
    pub fn new(bytes: Box<[u8]>) -> Self {
        Self {
            bytes,
            glyph_cache: GlyphCache::new(),
            glyph_cache_weight: 0,
        }
    }

    /// Get a [`GlyphRenderer`] from this font.
    ///
    /// Passing `None` as render color will create a renderer suitable for
    /// computing only the width of the text. No texture will be created in
    /// mode.
    pub fn renderer(&mut self, color: Option<RGBA8>, font_size: usize) -> GlyphRenderer {
        let mut font_face = Face::parse(&self.bytes, 0).unwrap();

        if false {
            font_face.set_variation(WGHT, 900.0);
        }

        GlyphRenderer {
            font_face,
            glyph_cache: &mut self.glyph_cache,
            glyph_cache_weight: &mut self.glyph_cache_weight,
            render_data: color.map(|c| (Vec::new(), c)),
            font_size,
            width: 0,
        }
    }
}

impl<'a> GlyphRenderer<'a> {
    fn extract_glyph(
        &mut self,
        glyph: char,
        _next_glyph: Option<char>,
    ) -> (usize, usize, Rc<GrayScalePixelBuffer>) {
        let font_size_f32 = self.font_size as f32;

        let font_height = self.font_face.height() as f32;

        let glyph_id = self.font_face.glyph_index(glyph);
        if glyph_id.is_none() || font_size_f32 == 0.0 || font_height == 0.0 {
            log::error!("Font does not contain glyph {:?}", glyph);
            return (0, 0, failed_glyph(self.font_size));
        }

        let glyph_id = glyph_id.unwrap();
        let scaler = font_height / font_size_f32;

        let orig_h_advance = self.font_face.glyph_hor_advance(glyph_id).unwrap_or(self.font_size as u16);
        let h_advance_scaled = (orig_h_advance as f32) / scaler;

        let h_bearing = self.font_face.glyph_hor_side_bearing(glyph_id).unwrap_or(0);
        let h_bearing_scaled = (h_bearing as f32) / scaler;

        let h_advance = h_advance_scaled.round() as usize;
        let h_bearing = h_bearing_scaled.round() as usize;

        let glyph_mask = match self.glyph_cache.get(&(glyph, self.font_size)) {
            Some(glyph_mask) => glyph_mask.clone(),
            None => {
                let unscaled = Vec2::new(0.0, self.font_face.ascender() as f32);
                let mut outline = Outline::new(unscaled, scaler);
                if let None = self.font_face.outline_glyph(glyph_id, &mut outline) {
                    log::error!("Coudn't outline glyph {:?}", glyph);
                    return (0, 0, failed_glyph(self.font_size));
                }
                let segments = outline.finish();

                let width = h_advance;
                let height = self.font_size;
                let len = width * height;
                let mut mask = Vec::with_capacity(len);
                mask.resize(len, 0);

                fill::<_, 8>(&segments, &mut mask, Vec2::new(width, height));

                let mask = mask.into_boxed_slice();
                let glyph_mask = Rc::new(GrayScalePixelBuffer::new(mask, width, height));

                *self.glyph_cache_weight += len;
                // log::info!("glyph_cache_weight: {}B", self.glyph_cache_weight);

                self.glyph_cache.insert((glyph, self.font_size), glyph_mask.clone());

                glyph_mask
            },
        };

        (h_advance, h_bearing, glyph_mask)
    }

    fn append(
        &mut self,
        text: &str,
    ) {
        let old_width = self.width;

        for glyph in text.chars() {
            if glyph == ' ' {
                self.width += space_width(self.font_size);
                continue;
            }

            let (advance, side_bearing, _) = self.extract_glyph(glyph, None);

            if APPLY_SIDE_BEARING && self.width > side_bearing {
                self.width -= side_bearing;
                self.width += interchar_width(self.font_size);
            }

            self.width += advance;
        }

        if let Some((pixels, _)) = &mut self.render_data {
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

            let mut px_offset = old_line_len;
            let diff = new_line_len - old_line_len;
            for _ in 0..self.font_size {
                pixels[px_offset..][..diff].fill(0);
                px_offset += new_line_len;
            }

            let mut cursor = old_width;
            for glyph in text.chars() {
                if glyph == ' ' {
                    // lifetime trick
                    let (pixels, _) = &mut self.render_data.as_mut().unwrap();

                    let advance = space_width(self.font_size);
                    let mut px_offset = cursor * 4;
                    for _ in 0..self.font_size {
                        pixels[px_offset..px_offset + (advance * 4)].fill(0);
                        px_offset += new_line_len;
                    }
                    cursor += advance;
                    continue;
                }

                let (advance, side_bearing, glyph_mask) = self.extract_glyph(glyph, None);

                if APPLY_SIDE_BEARING && cursor > side_bearing {
                    cursor -= side_bearing;
                    cursor += interchar_width(self.font_size);
                }

                // lifetime trick: re-borrowing pixels after method call
                let (pixels, color) = &mut self.render_data.as_mut().unwrap();

                let fake_fb = pixels[cursor * 4..].as_rgba_mut();

                let mut dst_offset = 0;
                let mut src_offset = 0;
                for _ in 0..self.font_size {
                    for x in 0..advance {
                        let dst = &mut fake_fb[dst_offset + x];
                        let src = glyph_mask.buffer(src_offset + x);
                        dst.r = (((src.a as u32) * (color.r as u32)) / 255) as u8;
                        dst.g = (((src.a as u32) * (color.g as u32)) / 255) as u8;
                        dst.b = (((src.a as u32) * (color.b as u32)) / 255) as u8;
                        dst.a = (((src.a as u32) * (color.a as u32)) / 255) as u8;
                    }
                    dst_offset += self.width;
                    src_offset += advance;
                }

                cursor += advance;
            }
        }
    }

    /// Add some text to this texture / width computation
    pub fn write<T: fmt::Display + ?Sized>(&mut self, text: &T) {
        core::write!(self, "{}", text).unwrap();
    }

    /// Get the width of all processed glyphs.
    pub fn width(self) -> usize {
        self.width
    }

    /// Retrieves the texture containing a rendering of processed glyphs.
    ///
    /// This panics if this renderer was configured for width computation only.
    pub fn texture(self) -> PixelSource {
        if let Some((pixels, _color)) = self.render_data {
            let pixel_buffer = RgbaPixelBuffer::new(pixels.into_boxed_slice(), self.width, self.font_size);
            PixelSource::TextureNoSSAA(Box::new(pixel_buffer))
        } else {
            panic!("StrTexture: No render color → no texture");
        }
    }
}

/// Utility to compute the size of a whitespace based on font size.
pub fn space_width(font_size: usize) -> usize {
    font_size / 4
}

fn interchar_width(font_size: usize) -> usize {
    font_size / 24
}

impl<'a> fmt::Write for GlyphRenderer<'a> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text);

        Ok(())
    }
}

type FontStorage = HashMap<CheapString, Font>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.storage[usize::from(m)];
    assert!(storage.is_none());
    assert_eq!(m, FONT_MUTATOR_INDEX.into());

    *storage = Some(Box::new(FontStorage::new()));

    Ok(())
}

pub fn load_font_bytes(app: &mut Application, asset: CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
    let storage = app.storage[FONT_MUTATOR_INDEX].as_mut().unwrap();
    let storage: &mut FontStorage = storage.downcast_mut().unwrap();

    storage.insert(asset, Font::new(bytes));

    Ok(())
}

fn parser(app: &mut Application, m: MutatorIndex, _: NodeKey, asset: CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
    assert_eq!(m, FONT_MUTATOR_INDEX.into());
    load_font_bytes(app, asset, bytes)
}

/// Tag-less Mutator which simply stores fonts
pub const FONT_MUTATOR: Mutator = Mutator {
    name: cheap_string("FontMutator"),
    xml_tag: None,
    xml_attr_set: None,
    xml_accepts_children: false,
    handlers: Handlers {
        initializer,
        parser,
        ..DEFAULT_HANDLERS
    },
};

pub fn get_font<'a>(storage: &'a mut Storage, font: &CheapString) -> Option<&'a mut Font> {
    let storage = storage[FONT_MUTATOR_INDEX].as_mut().unwrap();
    let storage: &mut FontStorage = storage.downcast_mut().unwrap();
    storage.get_mut(font)
}

struct Outline {
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
