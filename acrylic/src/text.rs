//! Paragraph, Placeholder, TextCursor, Unbreakable, FontState, xml_paragraph

use crate::app::Application;
use crate::app::ScratchBuffer;
use crate::style::Color;
use crate::bitmap::RGBA;
use crate::node::node_box;
use crate::node::Axis;
use crate::node::LengthPolicy;
use crate::node::LayerCaching;
use crate::node::RenderCache;
use crate::node::RenderReason;
use crate::node::Margin;
use crate::node::Node;
use crate::node::NodePathSlice;
use crate::node::NodeBox;
use crate::node::please_clone_vec;
use crate::node::Event;
use crate::node::EventType;
use crate::node::Direction;
use crate::font::Font;
use crate::font::get_glyph;
use crate::font::FontConfig;
use crate::status;
use crate::Spot;
use crate::Size;

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
// use core::ops::DerefMut;
use core::str::Chars;

use alloc::string::String;
use alloc::vec::Vec;

/// A wrapping container for glyphs which should
/// not be separated.
pub struct Unbreakable {
    pub glyphs: Vec<Option<NodeBox>>,
    pub text: String,
    pub spot_size: Size,
}

impl Node for Unbreakable {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(Self {
            glyphs: please_clone_vec(&self.glyphs),
            text: self.text.clone(),
            spot_size: self.spot_size,
        })
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

    fn children(&self) -> &[Option<NodeBox>] {
        &self.glyphs
    }

    fn children_mut(&mut self) -> &mut [Option<NodeBox>] {
        &mut self.glyphs
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

    fn tree_log(&self, _: &Application, _: usize) {
        // do nothin
    }
}

impl Clone for Unbreakable {
    fn clone(&self) -> Self {
        Self {
            glyphs: please_clone_vec(&self.glyphs),
            text: self.text.clone(),
            spot_size: self.spot_size,
        }
    }
}

impl Debug for Unbreakable {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Unbreakable")
            .field("text", &self.text)
            .field("spot_size", &self.spot_size)
            .finish()
    }
}

/// An empty, invisible node which has the size
/// of a specific glyph. Used to occupy space
/// when it is too early to produce bitmaps of glyphs.
#[derive(Debug, Clone)]
pub struct Placeholder {
    pub ratio: f64,
    pub spot_size: Size,
}

impl Node for Placeholder {
    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn describe(&self) -> String {
        String::from("Loading glyphs...")
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }
}

/// Initially a font name, which is replaced
/// by a handle to the font once the font is
/// resolved.
#[derive(Debug, Clone)]
pub enum FontState {
    Available(usize),
    Pending(Option<String>),
}

impl FontState {
    pub fn unwrap(&self) -> usize {
        match self {
            FontState::Available(index) => *index,
            _ => panic!("unwrap called on a FontState::Pending"),
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
#[derive(Debug)]
pub struct Paragraph {
    pub parts: Vec<(FontConfig, Option<Color>, String)>,
    pub font: FontState,
    pub children: Vec<Option<NodeBox>>,
    pub space_width: usize,
    pub policy: Option<LengthPolicy>,
    pub fg_color: Color,
    pub margin: Option<Margin>,
    /// Ignored when `policy` is WrapContent.
    pub font_size: Option<usize>,
    pub cursors: Vec<TextCursor>,
    pub on_edit: Option<String>,
    pub on_submit: Option<String>,
    pub spot_size: Size,
    pub deployed: bool,
    pub render_cache: RenderCache,
    pub render_reason: RenderReason,
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
                weight: None,
                italic_angle: None,
                underline: None,
                overline: None,
                opacity: None,
                serif_rise: None,
            },
            color_override: None,
            chars: None,
        }
    }

    fn deploy(&mut self, rdr_cfg: Option<(usize, Color)>, app: &mut Application) -> Result<(), String> {
        let mut children = Vec::with_capacity(self.children.len());
        let default_unbreakable = Unbreakable {
            glyphs: Vec::new(),
            text: String::new(),
            spot_size: Size::zero(),
        };
        let mut unbreakable = default_unbreakable.clone();

        let font_index = self.font.unwrap();
        let font_bytes = &app.fonts[font_index];
        let font = match Font::from_slice(font_bytes, 0) {
            Ok(font) => Ok(font),
            Err(_) => Err(format!("could not parse font #{}", font_index)),
        }?;
        let g_cache = &mut app.glyph_cache;

        let mut next;
        let mut iter = self.into_iter();
        let mut current = iter.next();
        while let Some((char_cfg, color, c1)) = current {
            next = iter.next();
            if c1 == ' ' {
                let mut prev = default_unbreakable.clone();
                swap(&mut prev, &mut unbreakable);
                children.push(Some(node_box(prev)));
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
                let g_node = get_glyph(&font, font_index, g_cache, c1, c2, rdr_cfg, char_cfg);
                unbreakable.glyphs.push(Some(g_node));
                unbreakable.text.push(c1);
                if let None = next {
                    let mut prev = default_unbreakable.clone();
                    swap(&mut prev, &mut unbreakable);
                    children.push(Some(node_box(prev)));
                }
            }
            current = next;
        }
        self.children = children;
        Ok(())
    }

    pub fn get_height(&self) -> usize {
        let sz_h = self.spot_size.h;
        match self.policy {
            Some(LengthPolicy::Chunks(h)) => h,
            _ => match self.margin {
                Some(m) => sz_h.checked_sub(m.top + m.bottom).unwrap_or(0),
                None => sz_h,
            },
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
    fn tick(
        &mut self,
        app: &mut Application,
        _path: NodePathSlice,
        style: usize,
        _scratch: ScratchBuffer,
    ) -> Result<bool, ()> {
        let color = app.theme.styles[style].foreground;

        if !self.deployed || color != self.fg_color {
            let height = self.get_height();
            self.fg_color = color;
            app.should_recompute = true;
            self.deploy(Some((height, color)), app).unwrap();
            self.deployed = true;
        }

        self.render_reason.downgrade();

        if !self.render_reason.is_valid() {
            for c in 0..self.cursors.len() {
                let cursor = &self.cursors[c];
                if let Some((last_change_ms, _, _)) = cursor.blink_state {
                    let interval = cursor.blink_interval.unwrap_or(usize::MAX);
                    if app.instance_age_ms - last_change_ms < interval {
                        continue;
                    }
                }
                self.render_reason = RenderReason::Computation;
                break;
            }
        }

        Ok(self.render_reason.is_valid())
    }

    /*fn render_background(
        &mut self,
        app: &mut Application,
        _path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {

        spot.fill([0; RGBA]);

        Ok(())
    }*/

    fn render_foreground(
        &mut self,
        app: &mut Application,
        _path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        let height = self.get_height();
        let color = app.theme.styles[style].foreground;

        spot.fill([0; RGBA], false);
        for c in 0..self.cursors.len() {
            let cursor = &self.cursors[c];
            if let Some((last_change_ms, _, _)) = cursor.blink_state {
                let interval = cursor.blink_interval.unwrap_or(usize::MAX);
                if app.instance_age_ms - last_change_ms < interval {
                    continue;
                }
            }

            let (top_left, _) = spot.inner_crop(true).unwrap();
            let mut cursor_spot = (top_left, Size::zero(), None);
            let mut u_placer = self.cursor(top_left).unwrap();
            if let Some(mut i) = cursor.position {
                i += 1;
                'outer: for unbreakable in self.children() {
                    let unbreakable = unbreakable.as_ref().unwrap();
                    let (unbreakable_tl, _, _) = u_placer.advance(unbreakable);
                    let mut g_placer = unbreakable.cursor(unbreakable_tl).unwrap();
                    for glyph in unbreakable.children() {
                        let glyph = glyph.as_ref().unwrap();
                        cursor_spot = g_placer.advance(glyph);
                        if i == 0 {
                            break 'outer;
                        }
                        i -= 1;
                    }
                    if i == 0 {
                        cursor_spot.0.x += cursor_spot.1.w as isize;
                        break 'outer;
                    } else {
                        i -= 1;
                    }
                }
            }
            cursor_spot.1 = Size::new(2, height);
            spot.set_window(cursor_spot);

            // now borrowing mutably
            let cursor = &mut self.cursors[c];
            if cursor.blink_state.is_none() {
                cursor.blink_state = Some((0, false, Vec::new()));
            }

            let (last_change, shown, _) = cursor.blink_state.as_mut().unwrap();

            if *shown {
                spot.fill([0; RGBA], true);
            } else {
                spot.fill(color, true);
            }

            *last_change = app.instance_age_ms;
            *shown = !*shown;
        }

        Ok(())
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }

    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::FOREGROUND
    }

    fn handle(
        &mut self,
        _app: &mut Application,
        _: NodePathSlice,
        event: &Event,
    ) -> Result<Option<String>, ()> {
        let mut result = Ok(None);
        if let Event::FocusGrab(grabbed) = event {
            self.cursors.clear();
            if *grabbed {
                // TODO
                self.cursors.push(TextCursor {
                    position: None,
                    blink_interval: Some(800),
                    blink_state: None,
                });
            }
        }
        if let Event::TextReplace(text) = event {
            self.deployed = false;
            self.parts.clear();
            self.parts.push((FontConfig::default(), None, text.clone()));
            result = Ok(self.on_edit.clone());
        }
        if let Event::TextInsert(text) = event {
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

        // force cursor to show
        for cursor in self.cursors.iter_mut() {
            cursor.blink_state = None;
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

    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), String> {
        if let FontState::Pending(name) = &self.font {
            let mut index = 0;
            if let Some(name) = name {
                if let Some(font_index) = app.font_ns.get(name) {
                    index = *font_index;
                } else {
                    return Err(format!("unknown font: \"{}\"", name));
                }
            }
            if let None = app.fonts.get(index) {
                let default_name = "<default>".into();
                let msg = name.as_ref().unwrap_or(&default_name);
                return Err(format!("invalid font index: {} / {}", index, msg));
            }
            self.font = FontState::Available(index);
        }
        self.font_size = Some(self.font_size.unwrap_or(app.default_font_size));
        self.policy = {
            let err_msg = format!("paragraph must be in a container");
            let max = path.len() - 1;
            let parent = app.get_node(&path[..max].to_vec()).ok_or(err_msg.clone())?;
            let (parent_axis, _) = parent.container().ok_or(err_msg)?;

            Some(match parent_axis {
                Axis::Vertical => LengthPolicy::Chunks(self.font_size.unwrap()),
                Axis::Horizontal => LengthPolicy::WrapContent,
            })
        };

        self.deploy(None, app).unwrap();
        app.should_recompute = true;
        Ok(())
    }

    fn please_clone(&self) -> NodeBox {
        node_box(Self {
            parts: self.parts.clone(),
            font: self.font.clone(),
            children: please_clone_vec(&self.children),
            space_width: self.space_width.clone(),
            policy: self.policy.clone(),
            fg_color: self.fg_color.clone(),
            margin: self.margin.clone(),
            font_size: self.font_size.clone(),
            cursors: self.cursors.clone(),
            on_edit: self.on_edit.clone(),
            on_submit: self.on_submit.clone(),
            spot_size: self.spot_size.clone(),
            deployed: self.deployed.clone(),
            render_cache: self.render_cache.clone(),
            render_reason: self.render_reason.clone(),
        })
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

    fn children(&self) -> &[Option<NodeBox>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Option<NodeBox>] {
        &mut self.children
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

    fn validate_spot_size(&mut self, prev_size: Size) {
        if let Some(LengthPolicy::WrapContent) = self.policy {
            if self.spot_size.h != prev_size.h {
                self.deployed = false;
            }
        }

        self.render_reason = RenderReason::Resized;
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
) -> Result<Option<NodeBox>, String> {
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

    let spot_size = Size::zero();
    let paragraph = node_box(Paragraph {
        parts: {
            let mut vec = Vec::new();
            vec.push((FontConfig::default(), None, text?));
            vec
        },
        font: FontState::Pending(font),
        children: Vec::new(),
        space_width: 6,
        policy: None,
        cursors: Vec::new(),
        on_edit,
        on_submit,
        fg_color: [0; 4],
        font_size,
        margin,
        spot_size,
        deployed: false,
        render_cache: [None, None],
        render_reason: RenderReason::Resized,
    });

    Ok(Some(paragraph))
}
