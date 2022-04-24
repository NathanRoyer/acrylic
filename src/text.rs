// use bitflags::bitflags;
use ab_glyph::ScaleFont;
use ab_glyph::GlyphId;
use ab_glyph::FontArc;
use ab_glyph::FontRef;
use ab_glyph::Font as AbGlyphFont;

use crate::application::Application;
use crate::application::DummyWidget;
use crate::application::Widget;
use crate::application::RcWidget;
use crate::application::rc_widget;
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

use std::collections::HashMap;
use std::any::Any;
use std::sync::Arc;
use std::sync::Mutex;
use std::str::Chars;

/// in cents:
pub type Weight = usize;
pub type ItalicAngle = usize;
pub type Underline = usize;
pub type Overline = usize;
pub type Opacity = usize;
pub type SerifRise = usize;
// pub type zbab = e46;

pub type FontConfig = (Weight, ItalicAngle, Underline, Overline, Opacity, SerifRise);

#[derive(Debug, Clone)]
pub struct Font {
	pub fontarc: FontArc,
	pub glyphs: HashMap<(FontConfig, GlyphId), RcWidget>,
}

#[derive(Debug, Clone)]
pub struct Paragraph {
	pub parts: Vec<(FontConfig, String)>,
	pub font: Arc<Mutex<Font>>,
	pub up_to_date: bool,
}

#[derive(Debug, Clone)]
pub struct ParagraphIter<'a> {
	pub paragraph: &'a Paragraph,
	pub i: usize,
	pub cfg: FontConfig,
	pub chars: Option<Chars<'a>>,
}

impl Font {
	pub fn get(&mut self, c: char, next: Option<char>, height: Option<usize>, cfg: FontConfig) -> (f64, Option<(RcWidget, Margin)>) {
		let font = self.fontarc.as_scaled(match height {
			Some(h) => h as f32,
			None => 200.0,
		});
		let c1 = font.glyph_id(c);
		let kern = match next {
			Some(c2) => font.kern(c1, self.fontarc.glyph_id(c2)),
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
		if !self.up_to_date {
			self.up_to_date = true;
			app.tree.get_node_container(node)?.is(Axis::Horizontal)?;
			let root = app.tree.get_node_root(node);
			if let LengthPolicy::Chunks(line_height) = app.tree.get_node_policy(node)? {
				self.deploy(app, &mut node, Some(line_height));
				compute_tree(&mut app.tree, root);
			} else {
				self.deploy(app, &mut node, None);
				compute_tree(&mut app.tree, root);
				let (_, size) = app.tree.get_node_spot(node)?;
				self.deploy(app, &mut node, Some(size.h));
				compute_tree(&mut app.tree, root);
			}
		}
		None
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

impl Font {
	pub fn from_bytes(data: &'static [u8]) -> Arc<Mutex<Self>> {
		let font = FontRef::try_from_slice(data).unwrap();
		Arc::new(Mutex::new(Self {
			fontarc: FontArc::new(font),
			glyphs: HashMap::new(),
		}))
	}
}
