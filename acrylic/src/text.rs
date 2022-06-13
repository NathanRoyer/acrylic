use ab_glyph::Font as AbGlyphFont;
use ab_glyph::FontVec;
use ab_glyph::GlyphId;
use ab_glyph::ScaleFont;

use crate::app::for_each_line;
use crate::app::Application;
use crate::style::Color;
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
use crate::node::Event;
use crate::node::EventType;
use crate::node::Direction;
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
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
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

/// Paragraphs can show a cursor on top of the text.
#[derive(Debug, Clone)]
pub struct TextCursor {
    /// - `None` => before any char
    /// - `Some(N)` => after Nth char
    pub position: Option<usize>,
    /// in milliseconds
    pub blink_interval: Option<usize>,
    /// Please initialize to `None`
    pub blink_state: Option<(usize, bool, Vec<u8>)>,
}

/// A Paragraph represent a block of text. It can be
/// made of multiple parts which may have different
/// configurations: some might be underlined, some
/// might be bold, others can be both, etc.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub parts: Vec<(FontConfig, Option<Color>, String)>,
    pub font: FontState,
    pub children: Vec<RcNode>,
    pub space_width: usize,
    pub policy: Option<LengthPolicy>,
    /// Used in [`Paragraph::validate_spot`]
    pub prev_spot: Spot,
    pub fg_color: Color,
    pub margin: Option<Margin>,
    /// Ignored when `policy` is WrapContent.
    pub font_size: Option<usize>,
    pub cursors: Vec<TextCursor>,
    pub on_edit: Option<String>,
    pub on_submit: Option<String>,
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
    pub color_override: Option<Color>,
    pub chars: Option<Chars<'a>>,
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
            color_override: None,
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
        while let Some((char_cfg, color, c1)) = current {
            next = iter.next();
            if c1 == ' ' {
                let mut prev = default_unbreakable.clone();
                swap(&mut prev, &mut unbreakable);
                children.push(rc_node(prev));
            } else {
                let c2 = match next {
                    Some((_, _, c)) => match c {
                        ' ' => None,
                        _ => Some(c),
                    },
                    None => None,
                };
                let rdr_cfg = match (rdr_cfg, color) {
                    (Some((h, _)), Some(c)) => Some((h, c)),
                    _ => rdr_cfg,
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

    pub fn get_height(&self, content_size: Size) -> usize {
        match self.policy {
            Some(LengthPolicy::Chunks(h)) => h,
            _ => content_size.h,
        }
    }
}

impl<'a> Iterator for ParagraphIter<'a> {
    type Item = (FontConfig, Option<Color>, char);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let None = self.chars {
                let (cfg, color_ovrd, part) = self.paragraph.parts.get(self.i)?;
                self.chars = Some(part.chars());
                self.cfg = *cfg;
                self.color_override = *color_ovrd;
                self.i += 1;
            }
            match self.chars.as_mut()?.next() {
                Some(c) => break Some((self.cfg, self.color_override, c)),
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
        let spot = status(self.get_content_spot_at(self.spot))?;
        let (_, size) = spot;
        let height = self.get_height(spot.1);
        let color = app.theme.styles[s].foreground;

        if self.repaint.contains(NeedsRepaint::FOREGROUND) {
            let (dst, pitch, _) = app.blit(&spot, BlitPath::Node(path))?;
            for_each_line(dst, size, pitch, |_, line_dst| {
                line_dst.fill(0);
            });
            self.repaint.remove(NeedsRepaint::FOREGROUND);
            if !self.deployed || color != self.fg_color {
                self.fg_color = color;
                app.should_recompute = true;
                self.deploy(Some((height, color)));
                self.deployed = true;
            }
            app.global_repaint.insert(NeedsRepaint::OVERLAY);
        }

        if self.repaint.contains(NeedsRepaint::OVERLAY) {
            for cursor in self.cursors.iter_mut() {
                cursor.blink_state = None;
            }
            self.repaint.remove(NeedsRepaint::OVERLAY);
        }

        for c in 0..self.cursors.len() {
            let cursor = &self.cursors[c];
            if let Some((last_change_ms, _, _)) = cursor.blink_state {
                let interval = cursor.blink_interval.unwrap_or(usize::MAX);
                if app.instance_age_ms - last_change_ms < interval {
                    continue;
                }
            }

            let cursor_spot = {
                let cursor_position;
                if let Some(mut i) = cursor.position {
                    let mut c_spot = spot;
                    let mut advance = true;
                    'outer: for u in self.children() {
                        let unbreakable = lock(u).unwrap();
                        for g in unbreakable.children() {
                            let glyph = lock(g).unwrap();
                            if i == 0 {
                                c_spot = glyph.get_spot();
                                break 'outer;
                            }
                            i -= 1;
                        }
                        if i == 0 {
                            advance = false;
                        } else {
                            i -= 1;
                        }
                    }
                    let (mut position, size) = c_spot;
                    if advance {
                        position.x += size.w as isize;
                    }
                    cursor_position = position;
                } else {
                    cursor_position = spot.0;
                }
                (cursor_position, Size::new(2, height))
            };
            let (_, cursor_size) = cursor_spot;
            let (dst, pitch, _) = app.blit(&cursor_spot, BlitPath::Overlay)?;

            // now borrowing mutably
            let cursor = &mut self.cursors[c];
            if cursor.blink_state.is_none() {
                cursor.blink_state = Some((0, false, Vec::new()));
            }

            let (last_change, shown, _) = cursor.blink_state.as_mut().unwrap();

            if *shown {
                for_each_line(dst, cursor_size, pitch, |_, line_dst| {
                    line_dst.fill(0);
                });
            } else {
                for_each_line(dst, cursor_size, pitch, |_, line_dst| {
                    for i in 0..line_dst.len() {
                        line_dst[i] = color[i % RGBA];
                    }
                });
            }

            *last_change = app.instance_age_ms;
            *shown = !*shown;
        }
        Ok(s)
    }

    fn handle(
        &mut self,
        app: &mut Application,
        _: &NodePath,
        event: &Event,
    ) -> Result<Option<String>, ()> {
        let mut result = Ok(None);
        if let Event::FocusGrab(grabbed) = event {
            self.cursors.clear();
            app.global_repaint.insert(NeedsRepaint::OVERLAY);
            if *grabbed {
                let mut position = None;
                if let Some((point, _)) = &app.focus {
                    let mut i: usize = 0;
                    let mut sub = 1;
                    'outer: for u in self.children() {
                        sub = 1;
                        let unbreakable = lock(u).unwrap();
                        let (u_pos, u_size) = unbreakable.get_spot();
                        let range = u_pos.y..(u_pos.y + (u_size.h as isize));
                        let correct_line = range.contains(&point.y);
                        if correct_line && u_pos.x > point.x {
                            sub = 2;
                            break;
                        }
                        for g in unbreakable.children() {
                            if correct_line {
                                let glyph = lock(g).unwrap();
                                let (g_pos, g_size) = glyph.get_spot();
                                let right_border = g_pos.x + (g_size.w as isize);
                                if right_border > point.x {
                                    break 'outer;
                                }
                            }
                            i += 1;
                        }
                        i += 1;
                        sub = 2;
                    }
                    position = i.checked_sub(sub);
                }
                self.cursors.push(TextCursor {
                    position,
                    blink_interval: Some(800),
                    blink_state: None,
                });
            }
        }
        if let Event::TextReplace(text) = event {
            self.repaint.insert(NeedsRepaint::FOREGROUND);
            self.deployed = false;
            self.parts.clear();
            self.parts.push((FontConfig::default(), None, text.clone()));
            result = Ok(self.on_edit.clone());
        }
        if let Event::TextInsert(text) = event {
            self.repaint.insert(NeedsRepaint::FOREGROUND);
            self.deployed = false;
            if self.parts.is_empty() {
                self.parts.push((FontConfig::default(), None, String::new()));
            }
            let cursor_pos = match status(self.cursors.first())?.position {
                Some(p) => p + 1,
                None => 0,
            };
            let mut insert_pos = cursor_pos;
            for (_, _, part) in self.parts.iter_mut() {
                if insert_pos <= part.len() {
                    part.insert_str(insert_pos, text.as_str());
                    break;
                } else {
                    insert_pos -= part.len();
                }
            }
            self.cursors.first_mut().unwrap().position = (cursor_pos + text.len()).checked_sub(1);
            result = Ok(self.on_edit.clone());
        }
        if let Event::TextDelete(mut delete) = event {
            self.repaint.insert(NeedsRepaint::FOREGROUND);
            self.deployed = false;
            let cursor_pos = match status(self.cursors.first())?.position {
                Some(p) => p + 1,
                None => 0,
            };
            let new_cursor_pos = (cursor_pos as isize + match delete > 0 {
                true => delete - 1,
                false => delete,
            }).clamp(0, isize::MAX);
            self.cursors.first_mut().unwrap().position = (new_cursor_pos as usize).checked_sub(1);
            let mut delete_pos = cursor_pos;
            let (mut p, mut g) = (0, None);
            for (_, _, part) in &self.parts {
                if delete_pos <= part.len() {
                    g = Some(delete_pos);
                    break;
                } else {
                    delete_pos -= part.len();
                    p += 1;
                }
            }
            let (mut p, mut g) = (p, status(g)?);
            loop {
                let (_, _, part) = status(self.parts.get_mut(p))?;
                let part_len = part.len() as isize;
                let base = g as isize;
                let result = base + delete;
                let bound = result.clamp(0, part_len);
                let deleted = bound - base;
                let bound_u = bound as usize;
                let range = g.min(bound_u)..g.max(bound_u);
                part.replace_range(range, "");
                delete -= deleted;
                if result < 0 {
                    p -= 1;
                    g = status(self.parts.get(p))?.2.len();
                } else if result > part_len {
                    p += 1;
                    g = 0;
                } else {
                    break;
                }
            }
            result = Ok(self.on_edit.clone());
        }
        if let Event::DirInput(direction) = event {
            app.global_repaint.insert(NeedsRepaint::OVERLAY);
            let mut cursor_pos = match status(self.cursors.first())?.position {
                Some(p) => p as isize + 1,
                None => 0,
            };
            cursor_pos += match direction {
                Direction::Left => -1,
                Direction::Right => 1,
                _ => 0,
            };
            let cursor_pos: usize = status(cursor_pos.try_into().ok())?;
            self.cursors.first_mut().unwrap().position = cursor_pos.checked_sub(1);
        }
        if let Event::QuickAction1 = event {
            result = Ok(self.on_submit.clone());
        }
        result
    }

    fn supported_events(&self) -> EventType {
        let mut sup_events =
            EventType::FOCUS_GRAB |
            EventType::TEXT_REPLACE |
            EventType::TEXT_INSERT |
            EventType::TEXT_DELETE |
            EventType::DIR_INPUT;
        if self.on_submit.is_some() {
            sup_events |= EventType::QUICK_ACTION_1;
        } else if self.on_edit.is_none() {
            sup_events = EventType::empty();
        }
        sup_events
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
        for (_, _, part) in &self.parts {
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
/// The `on-edit` attribute is optional and specifies an
/// event handler to call when the textual content is edited by
/// the user.
/// See [`Application::add_handler`] to set event handlers up.
///
/// The `on-submit` attribute is optional and specifies an
/// event handler to call when the user validates the content of
/// the text box, for instance by pressing `Enter`.
/// See [`Application::add_handler`] to set event handlers up.
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
    let mut on_edit = None;
    let mut on_submit = None;

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "margin" => {
                let m = value.parse().map_err(|_| format!("bad value: {}", value))?;
                margin = Some(Margin::quad(m));
            }
            "txt" => text = Ok(value.clone()),
            "font" => font = Some(value.clone()),
            "on-edit" => on_edit = Some(value.clone()),
            "on-submit" => on_submit = Some(value.clone()),
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

    let spot = (Point::zero(), Size::zero());
    let paragraph = rc_node(Paragraph {
        parts: {
            let mut vec = Vec::new();
            vec.push((FontConfig::default(), None, text?));
            vec
        },
        font: FontState::Pending(font),
        children: Vec::new(),
        space_width: 10,
        policy: None,
        cursors: Vec::new(),
        on_edit,
        on_submit,
        fg_color: [0; 4],
        font_size,
        margin,
        spot,
        prev_spot: spot,
        deployed: false,
        repaint: NeedsRepaint::all(),
    });

    Ok(Some(paragraph))
}
