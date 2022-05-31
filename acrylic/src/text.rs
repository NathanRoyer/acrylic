use ab_glyph::Font as AbGlyphFont;
use ab_glyph::FontVec;
use ab_glyph::GlyphId;
use ab_glyph::ScaleFont;

use crate::app::for_each_line;
use crate::app::Application;
use crate::app::Color;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::geometry::aspect_ratio;
use crate::lock;
use crate::node::rc_node;
use crate::node::Axis;
use crate::node::LengthPolicy;
use crate::node::Margin;
use crate::node::NeedsRepaint;
use crate::node::Node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::status;
use crate::BlitPath;
use crate::Point;
use crate::Size;
use crate::Spot;

#[cfg(feature = "xml")]
use crate::format;
#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::TreeParser;

use core::any::Any;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;
use core::mem::swap;
use core::ops::DerefMut;
use core::str::Chars;

use std::collections::HashMap;
use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

/// 1/100 of a value
pub type Cents = usize;

/// Specifies a font variant
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct FontConfig {
    pub weight: Cents,
    pub italic_angle: Cents,
    pub underline: Cents,
    pub overline: Cents,
    pub opacity: Cents,
    pub serif_rise: Cents,
}

/// The Font object contains font data as well
/// as a cache of previously rendered glyphs.
#[derive(Debug)]
pub struct Font {
    pub(crate) ab_glyph_font: FontVec,
    pub(crate) glyphs: HashMap<(usize, Color, FontConfig, GlyphId), RcNode>,
}

/// A handle to a [`Font`].
pub type RcFont = Arc<Mutex<Font>>;

/// A wrapping container for glyphs which should
/// not be separated.
#[derive(Clone)]
pub struct Unbreakable {
    pub glyphs: Vec<RcNode>,
    pub text: String,
    pub spot: Spot,
}

impl Node for Unbreakable {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        self.text.clone()
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::WrapContent
    }

    fn container(&self) -> Option<(Axis, usize)> {
        Some((Axis::Horizontal, 0))
    }

    fn children(&self) -> &[RcNode] {
        &self.glyphs
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
    }
}

impl Debug for Unbreakable {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Unbreakable")
            .field("text", &self.text)
            .field("spot", &self.spot)
            .finish()
    }
}

/// A single glyph. The underlying bitmap is shared
/// among all instances of that glyph.
#[derive(Debug, Clone)]
pub struct GlyphNode {
    pub bitmap: RcNode,
    pub spot: Spot,
    pub repaint: NeedsRepaint,
}

impl Node for GlyphNode {
    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        _: usize,
    ) -> Result<usize, ()> {
        if self.repaint.contains(NeedsRepaint::FOREGROUND) {
            let mut bitmap = status(lock(&self.bitmap))?;
            let bitmap = bitmap.deref_mut().as_any();
            let bitmap = status(bitmap.downcast_mut::<Bitmap>())?;
            bitmap.render_at(app, path, self.spot)?;
            self.repaint.remove(NeedsRepaint::FOREGROUND);
        }
        Ok(0)
    }

    fn policy(&self) -> LengthPolicy {
        // that unwrap is ugly...
        let mut bitmap = lock(&self.bitmap).unwrap();
        bitmap.deref_mut().policy()
    }

    fn repaint_needed(&mut self, repaint: NeedsRepaint) {
        self.repaint.insert(repaint);
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.repaint = NeedsRepaint::all();
        self.spot = spot;
    }

    fn describe(&self) -> String {
        String::from("Glyph")
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// An empty, invisible node which has the size
/// of a specific glyph. Used to occupy space
/// when it is too early to produce bitmaps of glyphs.
#[derive(Debug, Clone)]
pub struct Placeholder {
    pub ratio: f64,
    pub spot: Spot,
}

impl Node for Placeholder {
    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn describe(&self) -> String {
        String::from("Loading glyphs...")
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
    }
}

/// Initially a font name, which is replaced
/// by a handle to the font once the font is
/// resolved.
#[derive(Debug, Clone)]
pub enum FontState {
    Available(Arc<Mutex<Font>>),
    Pending(Option<String>),
}

impl FontState {
    pub fn unwrap(&self) -> &Arc<Mutex<Font>> {
        match self {
            FontState::Available(arc) => arc,
            _ => panic!("unwrap called on a FontState::Pending"),
        }
    }
}

/// A Paragraph represent a block of text. It can be
/// made of multiple parts which may have different
/// configurations: some might be underlined, some
/// might be bold, others can be both, etc.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub parts: Vec<(FontConfig, String)>,
    pub font: FontState,
    pub children: Vec<RcNode>,
    pub space_width: usize,
    pub policy: Option<LengthPolicy>,
    /// Used in [`Paragraph::validate_spot`]
    pub prev_spot: Spot,
    pub margin: Option<Margin>,
    /// Ignored when `policy` is WrapContent.
    pub font_size: Option<usize>,
    pub spot: Spot,
    pub deployed: bool,
    /// Initialize to `true`
    pub repaint: NeedsRepaint,
}

#[derive(Debug, Clone)]
struct ParagraphIter<'a> {
    pub paragraph: &'a Paragraph,
    pub i: usize,
    pub cfg: FontConfig,
    pub chars: Option<Chars<'a>>,
}

impl Font {
    /// Parse a TTF / OpenType font's data
    pub fn from_bytes(data: Vec<u8>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            ab_glyph_font: FontVec::try_from_vec(data).unwrap(),
            glyphs: HashMap::new(),
        }))
    }

    /// Used internally to obtain a rendered glyph
    /// from the font, which is then kept in cache.
    pub fn get(
        &mut self,
        c: char,
        next: Option<char>,
        rdr_cfg: Option<(usize, Color)>,
        char_cfg: FontConfig,
    ) -> RcNode {
        let font = self.ab_glyph_font.as_scaled(match rdr_cfg {
            Some((h, _)) => h as f32,
            None => 200.0,
        });
        let c1 = font.glyph_id(c);
        let kern = match next {
            Some(c2) => font.kern(c1, self.ab_glyph_font.glyph_id(c2)),
            _ => 0.0,
        };
        let glyph = font.scaled_glyph(c);
        let g_box = font.glyph_bounds(&glyph);
        let box_w = (g_box.width() + kern).ceil() as isize;
        let box_h = g_box.height().ceil() as isize;
        let ratio = aspect_ratio(box_w as usize, box_h as usize);
        if rdr_cfg.is_none() {
            rc_node(Placeholder {
                ratio,
                spot: (Point::zero(), Size::zero()),
            })
        } else if let Some(q) = font.outline_glyph(glyph) {
            let outline_bounds = q.px_bounds();
            let top = (outline_bounds.min.y - g_box.min.y).ceil() as isize;
            let left = (outline_bounds.min.x - g_box.min.x).ceil() as isize;
            let glyph_w = outline_bounds.width().ceil() as isize;
            let glyph_h = outline_bounds.height().ceil() as isize;
            let margin = Margin {
                top,
                left,
                right: box_w - (left + glyph_w),
                bottom: box_h - (top + glyph_h),
            };

            let (h, color) = rdr_cfg.unwrap();
            let rc_bitmap = if let Some(rc_bitmap) = self.glyphs.get(&(h, color, char_cfg, c1)) {
                rc_bitmap.clone()
            } else {
                let bmpsz = Size::new(glyph_w as usize, glyph_h as usize);
                let mut bitmap = Bitmap::new(bmpsz, RGBA, Some(margin));

                q.draw(|x, y, c| {
                    let (x, y) = (x as usize, y as usize);
                    let i = (y * bmpsz.w + x) * RGBA;
                    let mut pixel = color;
                    pixel[3] = (color[3] as f32 * c) as u8;
                    if let Some(slice) = bitmap.pixels.get_mut(i..(i + RGBA)) {
                        slice.copy_from_slice(&pixel);
                    }
                });

                let rc_bitmap = rc_node(bitmap);
                self.glyphs
                    .insert((h, color, char_cfg, c1), rc_bitmap.clone());
                rc_bitmap
            };
            rc_node(GlyphNode {
                bitmap: rc_bitmap,
                spot: (Point::zero(), Size::zero()),
                repaint: NeedsRepaint::all(),
            })
        } else {
            rc_node(Placeholder {
                ratio,
                spot: (Point::zero(), Size::zero()),
            })
        }
    }
}

impl Paragraph {
    fn into_iter(&self) -> ParagraphIter {
        ParagraphIter {
            paragraph: self,
            i: 0,
            cfg: FontConfig {
                weight: 0,
                italic_angle: 0,
                underline: 0,
                overline: 0,
                opacity: 0,
                serif_rise: 0,
            },
            chars: None,
        }
    }

    fn deploy(&mut self, rdr_cfg: Option<(usize, Color)>) {
        let mut children = Vec::with_capacity(self.children.len());
        let default_unbreakable = Unbreakable {
            glyphs: Vec::new(),
            text: String::new(),
            spot: (Point::zero(), Size::zero()),
        };
        let mut unbreakable = default_unbreakable.clone();
        let mut font = lock(&self.font.unwrap()).unwrap();

        let mut next;
        let mut iter = self.into_iter();
        let mut current = iter.next();
        while let Some((char_cfg, c1)) = current {
            next = iter.next();
            if c1 == ' ' {
                let mut prev = default_unbreakable.clone();
                swap(&mut prev, &mut unbreakable);
                children.push(rc_node(prev));
            } else {
                let c2 = match next {
                    Some((_, c)) => match c {
                        ' ' => None,
                        _ => Some(c),
                    },
                    None => None,
                };
                unbreakable.glyphs.push(font.get(c1, c2, rdr_cfg, char_cfg));
                unbreakable.text.push(c1);
                if let None = next {
                    let mut prev = default_unbreakable.clone();
                    swap(&mut prev, &mut unbreakable);
                    children.push(rc_node(prev));
                }
            }
            current = next;
        }
        self.children = children;
    }
}

impl<'a> Iterator for ParagraphIter<'a> {
    type Item = (FontConfig, char);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let None = self.chars {
                let (cfg, part) = self.paragraph.parts.get(self.i)?;
                self.chars = Some(part.chars());
                self.cfg = *cfg;
                self.i += 1;
            }
            match self.chars.as_mut()?.next() {
                Some(c) => break Some((self.cfg, c)),
                None => self.chars = None,
            }
        }
    }
}

impl Node for Paragraph {
    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        s: usize,
    ) -> Result<usize, ()> {
        if self.repaint.contains(NeedsRepaint::FOREGROUND) {
            let color = app.styles[s].foreground;
            let spot = status(self.get_content_spot_at(self.spot))?;
            let (dst, pitch, _) = app.blit(&spot, BlitPath::Node(path))?;
            let (_, size) = spot;
            for_each_line(dst, size, pitch, |_, line_dst| {
                line_dst.fill(0);
            });
            app.should_recompute = true;
            self.repaint.remove(NeedsRepaint::FOREGROUND);
            if !self.deployed {
                self.deploy(Some((
                    match self.policy {
                        Some(LengthPolicy::Chunks(h)) => h,
                        _ => size.h,
                    },
                    color,
                )));
                self.deployed = true;
            }
        }
        Ok(s)
    }

    fn margin(&self) -> Option<Margin> {
        self.margin
    }

    fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
        if let FontState::Pending(name) = &self.font {
            if let Some(font) = app.fonts.get(&name) {
                self.font = FontState::Available(font.clone());
            } else {
                let msg = format!("<app-default>");
                let name = name.as_ref().unwrap_or(&msg);
                Err(format!("unknown font: \"{}\"", name))?;
            }
        }
        self.font_size = Some(self.font_size.unwrap_or(app.default_font_size));
        self.policy = {
            let err_msg = format!("paragraph must be in a container");
            let max = path.len() - 1;
            let parent = app.get_node(&path[..max].to_vec()).ok_or(err_msg.clone())?;
            let parent = parent.lock().unwrap();
            let (parent_axis, _) = parent.container().ok_or(err_msg)?;

            Some(match parent_axis {
                Axis::Vertical => LengthPolicy::Chunks(self.font_size.unwrap()),
                Axis::Horizontal => LengthPolicy::WrapContent,
            })
        };

        self.deploy(None);
        app.should_recompute = true;
        app.blit_hooks
            .push((path.clone(), (Point::zero(), Size::zero())));
        Ok(())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        let mut legend = String::new();
        for (_, part) in &self.parts {
            legend += &part;
        }
        legend
    }

    fn container(&self) -> Option<(Axis, usize)> {
        Some((Axis::Horizontal, self.space_width))
    }

    fn policy(&self) -> LengthPolicy {
        self.policy.unwrap()
    }

    fn children(&self) -> &[RcNode] {
        &self.children
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
    }

    fn repaint_needed(&mut self, repaint: NeedsRepaint) {
        self.repaint.insert(repaint);
    }

    fn validate_spot(&mut self) {
        if let Some(LengthPolicy::WrapContent) = self.policy {
            if self.spot.1.h != self.prev_spot.1.h {
                self.deployed = false;
            }
        }
        if self.spot != self.prev_spot {
            self.repaint = NeedsRepaint::all();
        }
        self.prev_spot = self.spot;
    }
}

/// XML tag for paragraphs of text.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`Paragraph`] node.
///
/// A font's name is the one you specified in [`Application::add_font`].
///
/// ```xml
/// <p txt="Hello World!" font="some-font-name" font-size="20" margin="10" />
/// ```
///
/// The `txt` attribute is mandatory and must contain valid UTF-8.
///
/// The `font` attribute is optional and must point to a loaded font.
///
/// The `font-size` attribute is optional.
/// It is ignored if the paragraph ends up in an horizontal container.
///
/// The `margin` attribute is optional and specifies a margin around the paragraph.
///
/// It is impossible at the moment to use this for rich text, but it is
/// a planned feature.
#[cfg(feature = "xml")]
pub fn xml_paragraph(
    _: &mut TreeParser,
    attributes: &[Attribute],
) -> Result<Option<RcNode>, String> {
    let mut text = Err(String::from("missing txt attribute"));
    let mut font_size = None;
    let mut font = None;
    let mut margin = None;

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "margin" => {
                let m = value.parse().map_err(|_| format!("bad value: {}", value))?;
                margin = Some(Margin::quad(m));
            }
            "txt" => text = Ok(value.clone()),
            "font" => font = Some(value.clone()),
            "font-size" => {
                font_size = Some(
                    value
                        .parse()
                        .ok()
                        .ok_or(format!("bad font-size: {}", &value))?,
                )
            }
            _ => unexpected_attr(&name)?,
        }
    }

    let font_config = FontConfig {
        weight: 0,
        italic_angle: 0,
        underline: 0,
        overline: 0,
        opacity: 0,
        serif_rise: 0,
    };

    let spot = (Point::zero(), Size::zero());
    let paragraph = rc_node(Paragraph {
        parts: {
            let mut vec = Vec::new();
            vec.push((font_config, text?));
            vec
        },
        font: FontState::Pending(font),
        children: Vec::new(),
        space_width: 10,
        policy: None,
        font_size,
        margin,
        spot,
        prev_spot: spot,
        deployed: false,
        repaint: NeedsRepaint::all(),
    });

    Ok(Some(paragraph))
}
