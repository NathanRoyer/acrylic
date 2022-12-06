//! Paragraph, TextCursor, Unbreakable, FontState, xml_paragraph

use crate::app::Application;
use crate::app::ScratchBuffer;
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
use crate::font::Font;
use crate::font::get_glyph_mask;
use crate::font::FontConfig;
use crate::font::FontIndex;
use crate::Point;
use crate::Spot;
use crate::Size;

#[cfg(feature = "xml")]
use crate::xml::{invalid_attr_val, check_attr, unexpected_attr, Attribute, TreeParser};

use log::error;

use core::any::Any;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;
// use core::ops::DerefMut;

use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;

/// A wrapping container for glyphs which should
/// not be separated.
pub struct Unbreakable {
    pub text: String,
    pub spot_size: Size,
    pub width: usize,
    pub render_cache: RenderCache,
    pub render_reason: RenderReason,
    pub font_index: FontIndex,
    pub font_config: FontConfig,
}

impl Node for Unbreakable {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn describe(&self) -> String {
        self.text.clone()
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::Fixed(self.width)
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

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

    fn validate_spot_size(&mut self, _: Size) {
        self.render_reason = RenderReason::Resized;
    }

    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::FOREGROUND
    }

    fn render_foreground(
        &mut self,
        app: &mut Application,
        _path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if self.render_reason.is_valid() {
            let color = app.theme.styles[style].foreground;

            let font_bytes = &app.fonts.get(self.font_index).ok_or_else(|| {
                self.render_reason = RenderReason::Resized;
            })?;

            let font = Font::from_slice(font_bytes, 0).map_err(|e| {
                error!("Unbreakable: could not parse font #{}: {}", self.font_index, e)
            })?;

            let font_size = self.spot_size.h;
            let (top_left, window, margin) = spot.window;

            let mut cursor = 0;
            for glyph in self.text.chars() {
                let key = (self.font_index, font_size, self.font_config, glyph);
                let glyph_mask;

                if !app.glyph_cache.contains_key(&key) {
                    glyph_mask = get_glyph_mask(glyph, &font, self.font_config, font_size, None);
                    app.glyph_cache.insert(key, Arc::new(glyph_mask));
                }

                let (size, mask) = app.glyph_cache.get(&key).unwrap().as_ref();

                let pos = Point::new(top_left.x + (cursor as isize), top_left.y);
                spot.set_window((pos, *size, None));

                let mut src = &mask[..];
                spot.for_each_line(false, |_, mut dst| {
                    for opacity in &src[..size.w] {
                        let opacity = *opacity as u32;
                        dst[0] = color[0];
                        dst[1] = color[1];
                        dst[2] = color[2];
                        dst[3] = ((color[3] as u32 * opacity) / 255) as u8;
                        dst = &mut dst[RGBA..];
                    }

                    src = &src[size.w..];
                });

                cursor += size.w;
            }

            if self.width != cursor {
                app.should_recompute = true;
                self.width = cursor;
            }

            spot.set_window((top_left, window, margin));
        }
        Ok(())
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }
}

impl Clone for Unbreakable {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            spot_size: self.spot_size.clone(),
            width: self.width.clone(),
            render_cache: self.render_cache.clone(),
            render_reason: self.render_reason.clone(),
            font_index: self.font_index.clone(),
            font_config: self.font_config.clone(),
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
    pub parts: Vec<Option<NodeBox>>,
    pub space_width: usize,
    pub policy: Option<LengthPolicy>,
    pub margin: Option<Margin>,
    /// Ignored when `policy` is WrapContent.
    pub font_size: Option<usize>,
    pub on_edit: Option<String>,
    pub on_submit: Option<String>,
    pub spot_size: Size,
}

impl Paragraph {
    pub fn new(
        font_size: Option<usize>,
        on_submit: Option<String>,
        on_edit: Option<String>,
        margin: Option<Margin>,
    ) -> Self {
        Self {
            parts: Vec::new(),
            space_width: 6,
            policy: None,
            on_edit,
            on_submit,
            font_size,
            margin,
            spot_size: Size::zero(),
        }
    }

    pub fn set_text(&mut self, text: String) {
        self.parts = text.split(" ").map(|part| {
            Some(node_box(Unbreakable {
                text: String::from(part),
                spot_size: Size::zero(),
                width: 0,
                render_cache: [None, None],
                render_reason: RenderReason::Resized,
                font_index: 0,
                font_config: FontConfig::default(),
            }))
        }).collect();
    }
}

impl Node for Paragraph {
    fn margin(&self) -> Option<Margin> {
        self.margin
    }

    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), ()> {
        self.font_size = Some(self.font_size.unwrap_or(app.default_font_size));
        self.policy = {
            let parent_path = &path[..(path.len() - 1)];
            let parent_cont = app
                .get_node(parent_path)
                .and_then(|p| p.container());

            if let Some((parent_axis, _)) = parent_cont {
                Some(match parent_axis {
                    Axis::Vertical => LengthPolicy::Chunks(self.font_size.unwrap()),
                    Axis::Horizontal => LengthPolicy::WrapContent,
                })
            } else {
                Err(error!("Paragraph::initialize: paragraph must be in a container"))?
            }
        };

        Ok(())
    }

    fn please_clone(&self) -> NodeBox {
        node_box(Self {
            parts: please_clone_vec(&self.parts),
            space_width: self.space_width.clone(),
            policy: self.policy.clone(),
            margin: self.margin.clone(),
            font_size: self.font_size.clone(),
            on_edit: self.on_edit.clone(),
            on_submit: self.on_submit.clone(),
            spot_size: self.spot_size.clone(),
        })
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        String::from("Paragraph")
    }

    fn container(&self) -> Option<(Axis, usize)> {
        Some((Axis::Horizontal, self.space_width))
    }

    fn policy(&self) -> LengthPolicy {
        self.policy.unwrap()
    }

    fn children(&self) -> &[Option<NodeBox>] {
        &self.parts
    }

    fn children_mut(&mut self) -> &mut [Option<NodeBox>] {
        &mut self.parts
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
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
    line: usize,
    attributes: Vec<Attribute>,
) -> Result<Option<NodeBox>, ()> {
    const TN: &'static str = "p";
    let mut text = None;
    let mut font_size = None;
    let mut font = None;
    let mut margin = None;
    let mut on_edit = None;
    let mut on_submit = None;

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "margin" => {
                let m = value.parse().map_err(|_| invalid_attr_val(line, TN, "margin", &value))?;
                margin = Some(Margin::quad(m));
            }
            "txt" => text = Some(value),
            "font" => font = Some(value),
            "on-edit" => on_edit = Some(value),
            "on-submit" => on_submit = Some(value),
            "font-size" => {
                font_size = Some(
                    value.parse().map_err(|_| invalid_attr_val(line, TN, "margin", &value))?,
                )
            }
            _ => unexpected_attr(line, TN, &name)?,
        }
    }

    let _font = font;

    let mut paragraph = Paragraph::new(font_size, on_submit, on_edit, margin);
    paragraph.set_text(check_attr(line, TN, "txt", text)?);

    Ok(Some(node_box(paragraph)))
}
