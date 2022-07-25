use crate::app::Application;
use crate::app::ScratchBuffer;
use crate::style::Color;
use crate::node::node_box;
use crate::node::NodeBox;
use crate::node::Node;
use crate::node::NodePathSlice;
use crate::node::RenderCache;
use crate::node::RenderReason;
use crate::node::LayerCaching;
use crate::node::Margin;
use crate::node::LengthPolicy;
use crate::text::Placeholder;
use crate::NewSpot;
use crate::Size;

use crate::bitmap::aspect_ratio_with_m;
use crate::bitmap::blit_rgba;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;

use ttf_parser::OutlineBuilder;
use ttf_parser::GlyphId;
pub(crate) use ttf_parser::Face as Font;

use vek::vec::repr_c::vec2::Vec2;
use vek::bezier::repr_c::CubicBezier2;
use vek::bezier::repr_c::QuadraticBezier2;

use wizdraw::simplify;
use wizdraw::rasterize;

use hashbrown::hash_map::HashMap;

use core::any::Any;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::sync::Arc;

/// 1/100 of a value
pub type Hundredth = usize;

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
/// Key is `(font_index, px_height, color, config, character)`.
pub type GlyphCache = HashMap<(usize, usize, Color, FontConfig, char), Arc<Bitmap>>;

/// Used internally to obtain a rendered glyph
/// from the font, which is then kept in cache.
pub fn get_glyph(
    font: &Font,
    font_index: usize,
    glyphs: &mut GlyphCache,
    c: char,
    _next: Option<char>,
    rdr_cfg: Option<(usize, Color)>,
    _char_cfg: FontConfig,
) -> NodeBox {
    let mut retval = node_box(Placeholder {
        ratio: 0.0,
        spot_size: Size::zero(),
    });

    let (height, color) = match rdr_cfg {
        Some((h, c)) => (h, c),
        None => return retval,
    };

    let key = (font_index, height, color, _char_cfg, c);

    let glyph_id = font.glyph_index(c).unwrap_or(GlyphId(0));
    let font_height = font.height();
    let scaler = (font_height as f32) / (height as f32);

    if let Some(rect) = font.glyph_bounding_box(glyph_id) {
        let h_advance = font.glyph_hor_advance(glyph_id).unwrap_or(rect.width() as u16);
        let h_advance_f32 = h_advance as f32;
        let base = Vec2 {
            x: rect.x_min as f32,
            y: font.ascender() as f32,
        };

        // kerning is not supported yet
        // but some work have been done to ease its support
        /*if let Some(c2) = _next {
            let kerning_subtable: Vec<_> = font
                .tables()
                .kern
                .iter()
                .flat_map(|c| c.subtables)
                .filter(|st| st.horizontal)
                .collect();
            let gid2 = font.glyph_index(c).unwrap_or(GlyphId(0));
            let h_kern = kerning_subtable.iter()
                .find_map(|st| st.glyphs_kerning(glyph_id, gid2));
            if let Some(k) = h_kern {
                _app.log(&crate::format!("{} + {}: k={}", c, c2, k));
            }
        }*/

        let _lsb = font.glyph_hor_side_bearing(glyph_id).unwrap_or(0);
        let _rsb = (h_advance as i16) - (_lsb + rect.width());
        // _app.log(&crate::format!("{}: lsb={} rsb={}", c, _lsb, _rsb));
        let margin = Margin {
            top: 0,
            bottom: 0,
            left: (height / 12),
            right: 0,
        };

        let bitmap;
        if let Some(cached_bmp) = glyphs.get(&key) {
            bitmap = cached_bmp.clone();
        } else {
            // anti-aliaising
            let aa = 4usize;
            let aa_sq = aa.pow(2);

            let mut outline = Outline::new(base, scaler / (aa as f32));
            font.outline_glyph(glyph_id, &mut outline).unwrap();
            let segments = outline.unwrap();
            // _app.log(&crate::format!("{:?}", &segments));

            let size = Size::new((h_advance_f32 / scaler) as usize, height);
            let mut bmp = Bitmap::new(size, RGBA, None);
            let mut mask = Vec::with_capacity(size.w * size.h * (aa_sq));
            mask.resize(size.w * size.h * aa_sq, 0);
            let m_size = Vec2::from((size.w * aa, size.h * aa));
            rasterize(&segments, &mut mask, m_size, None);
            {
                let pixels = bmp.pixels.as_mut_slice();
                for y in 0..size.h {
                    for x in 0..size.w {
                        let m_x = x * aa;
                        let m_y = y * aa;
                        let p = (y * size.w + x) * RGBA;
                        let pixel = &mut pixels[p..];
                        pixel[..3].copy_from_slice(&color[..3]);
                        let mut alpha = 0;
                        for i in 0..aa {
                            for j in 0..aa {
                                let p = (m_y + i) * m_size.x + (m_x + j);
                                alpha += mask[p] / (aa_sq as u8);
                            }
                        }
                        let alpha = (alpha as u32) * (color[3] as u32);
                        pixel[3] = (alpha / 255) as u8;
                    }
                    if false {
                        // debug left bitmap boundary
                        let p = y * size.w * RGBA;
                        let pixel = &mut pixels[p..];
                        pixel[3] = 255;
                    }
                }
            }
            // bmp.update_cache(size, true);
            bitmap = Arc::new(bmp);
            glyphs.insert(key, bitmap.clone());
        }

        retval = node_box(GlyphNode {
            bitmap,
            spot_size: Size::zero(),
            margin: Some(margin),
            render_cache: [None, None],
            render_reason: RenderReason::Resized,
        });
    }

    retval
}

/// A single glyph. The underlying bitmap is shared
/// among all instances of that glyph.
#[derive(Debug, Clone)]
pub struct GlyphNode {
    pub bitmap: Arc<Bitmap>,
    pub spot_size: Size,
    pub margin: Option<Margin>,
    pub render_cache: RenderCache,
    pub render_reason: RenderReason,
}

impl Node for GlyphNode {
    fn tick(
        &mut self,
        _app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        _scratch: ScratchBuffer,
    ) -> Result<bool, ()> {
        self.render_reason.downgrade();
        Ok(self.render_reason.is_valid())
    }

    fn render_foreground(
        &mut self,
        _app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        spot: &mut NewSpot,
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if self.render_reason.is_valid() {
            blit_rgba::<true, 2>(
                &self.bitmap.pixels,
                self.bitmap.channels,
                self.bitmap.size,
                spot,
            );
        }
        Ok(())
    }

    fn validate_spot_size(&mut self, _: Size) {
        self.render_reason = RenderReason::Resized;
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }

    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::FOREGROUND
    }

    /// Nodes can report a margin to the layout algorithm
    /// via this method.
    fn margin(&self) -> Option<Margin> {
        self.margin
    }

    fn policy(&self) -> LengthPolicy {
        let ratio = aspect_ratio_with_m(self.bitmap.size, self.margin);
        LengthPolicy::AspectRatio(ratio)
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

    fn describe(&self) -> String {
        String::from("Glyph")
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
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

    pub fn unwrap(self) -> Vec<Vec2<f32>> {
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
        simplify::<_, 4>(&curve, 1.0, &mut self.points);
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
        simplify::<_, 4>(&curve, 1.0, &mut self.points);
        self.last_point = end;
    }

    fn close(&mut self) {
        if self.points.first().is_some() {
            self.points.push(self.points[0]);
        }
    }
}
