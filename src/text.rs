use ab_glyph::ScaleFont;
use ab_glyph::GlyphId;
use ab_glyph::FontVec;
use ab_glyph::Font as AbGlyphFont;

use crate::app::Application;
use crate::app::DummyWidget;
use crate::app::Widget;
use crate::app::RcWidget;
use crate::app::rc_widget;
use crate::geometry::aspect_ratio;
use crate::tree::LengthPolicy;
use crate::tree::NodeKey;
use crate::tree::Margin;
use crate::tree::Axis;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::flexbox::compute_tree;
use crate::Void;
use crate::Size;
use crate::Point;

#[cfg(feature = "xml")]
use crate::xml::Attribute;

use core::any::Any;
use core::str::Chars;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

pub type Cents = usize;

/// Weight of the text, times 0.01
pub type Weight = Cents;

/// Italic Angle of the text, times 0.01
pub type ItalicAngle = Cents;

/// Underline of the text, times 0.01
pub type Underline = Cents;

/// Overline of the text, times 0.01
pub type Overline = Cents;

/// Opacity of the text, times 0.01
pub type Opacity = Cents;

/// Serif rise of the text, times 0.01
pub type SerifRise = Cents;

pub type FontConfig = (Weight, ItalicAngle, Underline, Overline, Opacity, SerifRise);

/// The Font object contains font data as well
/// as a cache of previously rendered glyphs.
#[derive(Debug)]
pub struct Font {
	pub(crate) ab_glyph_font: FontVec,
	pub(crate) glyphs: HashMap<(FontConfig, GlyphId), RcWidget>,
}

/// A Paragraph represent a block of text. It can be
/// made of multiple parts which may have different
/// configurations: some might be underlined, some
/// might be bold, others can be both, etc.
///
/// TODO: handle font size changes properly.
#[derive(Debug, Clone)]
pub struct Paragraph {
	pub parts: Vec<(FontConfig, String)>,
	pub font: Arc<Mutex<Font>>,
	pub previous_size: Size,
}

#[derive(Debug, Clone)]
pub struct ParagraphIter<'a> {
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
	///
	/// TODO: handle font size changes properly.
	pub fn get(&mut self, c: char, next: Option<char>, height: Option<usize>, cfg: FontConfig) -> (f64, Option<(RcWidget, Margin)>) {
		let font = self.ab_glyph_font.as_scaled(match height {
			Some(h) => h as f32,
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
		let mut widget_margin = None;
		if height.is_none() {
			let widget = rc_widget(DummyWidget);
			let margin = Margin::new(0, 0, 0, 0);
			widget_margin = Some((widget, margin));
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

			let widget = if let Some(widget) = self.glyphs.get(&(cfg, c1)) {
				widget.clone()
			} else {
				let bmpsz = Size::new(glyph_w as usize, glyph_h as usize);
				let mut bmp = Bitmap::new(bmpsz, RGBA);

				q.draw(|x, y, c| {
					let (x, y) = (x as usize, y as usize);
					let i = (y * bmpsz.w + x) * RGBA;
					let a = (255.0 * c) as u8;
					if let Some(slice) = bmp.pixels.get_mut(i..(i + RGBA)) {
						slice.copy_from_slice(&[a, a, a, 255]);
					}
				});

				let bmp = rc_widget(bmp);
				self.glyphs.insert((cfg, c1), bmp.clone());
				bmp
			};
			widget_margin = Some((widget, margin))
		};
		(ratio, widget_margin)
	}
}

/// This function is to be used in [`crate::xml::TreeParser::with`].
#[cfg(feature = "xml")]
pub fn paragraph(app: &mut Application, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	let mut text = Err(String::from("missing txt attribute"));
	let mut font_size = app.default_font_size;
	let mut font = None;

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"txt" => text = Ok(value.clone()),
			"font" => font = Some(value.clone()),
			"font-size" => font_size = value.parse().ok().ok_or(format!("bad font-size: {}", &value))?,
			_ => Err(format!("unexpected attribute: {}", name))?,
		}
	}

	let err_msg = format!("unknown font: \"{}\"", font.as_ref().unwrap_or(&format!("<none>")));
	let font = app.fonts.get(&font).ok_or(err_msg)?.clone();

	let err_msg = String::from("paragraph must be in a container");
	let parent = parent.ok_or(err_msg.clone())?;
	let parent_axis = app.tree.get_node_container(*parent).ok_or(err_msg)?;

	let mut node = app.tree.add_node(Some(parent), 4);
	app.tree.set_node_widget(&mut node, Some(rc_widget(Paragraph {
		parts: vec![ ((0, 0, 0, 0, 0, 0), text?) ],
		font,
		previous_size: Size::zero(),
	})));
	app.tree.set_node_policy(&mut node, Some(match parent_axis {
		Axis::Vertical => LengthPolicy::Chunks(font_size),
		Axis::Horizontal => LengthPolicy::WrapContent(0, u32::MAX),
	}));
	app.tree.set_node_container(&mut node, Some(Axis::Horizontal));
	app.tree.set_node_spot(&mut node, Some((Point::zero(), Size::zero())));
	Ok(node)
}

impl Paragraph {
	fn into_iter(&self) -> ParagraphIter {
		ParagraphIter {
			paragraph: self,
			i: 0,
			cfg: (0, 0, 0, 0, 0, 0),
			chars: None,
		}
	}

	fn deploy(&mut self, app: &mut Application, node: &mut NodeKey, line_height: Option<usize>) {
		let children = app.tree.children(*node);
		let mut children = children.as_slice();

		let mut next;
		let mut iter = self.into_iter();
		let mut current = iter.next();
		while let Some((cfg, c1)) = current {
			next = iter.next();
			let child_before = if let Some(child) = children.first() {
				children = children.split_at(1).1;
				*child
			} else {
				// spot + widget + policy + margin = 4
				app.tree.add_node(Some(node), 4)
			};
			let mut child = child_before;
			let c2 = match next {
				Some((_, c)) => Some(c),
				None => None,
			};
			let mut font = self.font.lock().unwrap();
			let (r, widget_margin) = font.get(c1, c2, line_height, cfg);
			if let Some((widget, margin)) = widget_margin {
				app.tree.set_node_widget(&mut child, Some(widget));
				app.tree.set_node_margin(&mut child, Some(margin));
			}
			app.tree.set_node_policy(&mut child, Some(LengthPolicy::AspectRatio(r)));
			app.tree.set_node_spot(&mut child, Some((Point::zero(), Size::zero())));
			current = next;
		}
		for i in children {
			app.tree.del_node(*i, true);
		}
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

impl Widget for Paragraph {
	fn render(&mut self, app: &mut Application, mut node: NodeKey) -> Void {
		let size = app.tree.get_node_spot(node)?.1;
		if size != self.previous_size {
			self.previous_size = size;
			app.tree.get_node_container(node)?.is(Axis::Horizontal)?;
			let root = app.tree.get_node_root(node);
			if let LengthPolicy::Chunks(line_height) = app.tree.get_node_policy(node)? {
				self.deploy(app, &mut node, Some(line_height));
				compute_tree(&mut app.tree, root);
				self.previous_size = app.tree.get_node_spot(node)?.1;
			} else {
				self.deploy(app, &mut node, None);
				compute_tree(&mut app.tree, root);
				let (_, size) = app.tree.get_node_spot(node)?;
				self.deploy(app, &mut node, Some(size.h));
				compute_tree(&mut app.tree, root);
				self.previous_size = app.tree.get_node_spot(node)?.1;
			}
		}
		None
	}

	fn legend(&mut self, _: &mut Application, _: NodeKey) -> String {
		let mut legend = String::new();
		for (_, part) in &self.parts {
			legend += &part;
		}
		legend
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}
