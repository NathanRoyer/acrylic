use ab_glyph::ScaleFont;
use ab_glyph::GlyphId;
use ab_glyph::FontVec;
use ab_glyph::Font as AbGlyphFont;

use crate::app::Application;
use crate::node::Node;
use crate::node::RcNode;
use crate::node::NodePath;
use crate::node::rc_node;
use crate::geometry::aspect_ratio;
use crate::node::LengthPolicy;
use crate::node::Margin;
use crate::node::Axis;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::Spot;
use crate::Void;
use crate::Size;
use crate::Point;
use crate::lock;

#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::format;

use core::any::Any;
use core::str::Chars;
use core::mem::swap;
use core::ops::DerefMut;
use core::fmt::Debug;
use core::fmt::Result as FmtResult;
use core::fmt::Formatter;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::string::String;
use std::vec::Vec;

pub type Cents = usize;

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
	pub(crate) glyphs: HashMap<(usize, FontConfig, GlyphId), RcNode>,
}

pub type RcFont = Arc<Mutex<Font>>;

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

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.spot = spot;
		None
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

#[derive(Debug, Clone)]
pub struct GlyphNode {
	pub bitmap: RcNode,
	pub spot: Spot,
	pub dirty: bool,
}

impl Node for GlyphNode {
	fn render(&mut self, app: &mut Application, path: &mut NodePath) -> Void {
		if self.dirty {
			self.dirty = false;
			let mut bitmap = lock(&self.bitmap)?;
			let bitmap = bitmap.deref_mut().as_any();
			bitmap.downcast_mut::<Bitmap>()?.render_at(app, path, self.spot);
		}
		Some(())
	}

	fn policy(&self) -> LengthPolicy {
		// that unwrap is ugly...
		let mut bitmap = lock(&self.bitmap).unwrap();
		bitmap.deref_mut().policy()
	}

	fn set_dirty(&mut self) {
		self.dirty = true;
	}

	fn get_spot(&self) -> Spot {
		self.spot
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.dirty = true;
		self.spot = spot;
		None
	}

	fn describe(&self) -> String {
		String::from("Glyph")
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

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

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.spot = spot;
		None
	}
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
	pub children: Vec<RcNode>,
	pub space_width: usize,
	pub policy: LengthPolicy,
	pub previous_height: usize,
	pub margin: Option<Margin>,
	pub spot: Spot,
	pub dirty: bool,
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
	pub fn get(&mut self, c: char, next: Option<char>, height: Option<usize>, cfg: FontConfig) -> RcNode {
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
		if height.is_none() {
			rc_node(Placeholder { ratio, spot: (Point::zero(), Size::zero()) })
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

			let h = height.unwrap();
			let rc_bitmap = if let Some(rc_bitmap) = self.glyphs.get(&(h, cfg, c1)) {
				rc_bitmap.clone()
			} else {
				let bmpsz = Size::new(glyph_w as usize, glyph_h as usize);
				let mut bitmap = Bitmap::new(bmpsz, RGBA, Some(margin));

				q.draw(|x, y, c| {
					let (x, y) = (x as usize, y as usize);
					let i = (y * bmpsz.w + x) * RGBA;
					let a = (255.0 * c) as u8;
					if let Some(slice) = bitmap.pixels.get_mut(i..(i + RGBA)) {
						slice.copy_from_slice(&[255, 255, 255, a]);
					}
				});

				let rc_bitmap = rc_node(bitmap);
				self.glyphs.insert((h, cfg, c1), rc_bitmap.clone());
				rc_bitmap
			};
			rc_node(GlyphNode {
				bitmap: rc_bitmap,
				spot: (Point::zero(), Size::zero()),
				dirty: true,
			})
		} else {
			rc_node(Placeholder { ratio, spot: (Point::zero(), Size::zero()) })
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

	fn deploy(&mut self, line_height: Option<usize>) {
		let mut children = Vec::with_capacity(self.children.len());
		let default_unbreakable = Unbreakable {
			glyphs: Vec::new(),
			text: String::new(),
			spot: (Point::zero(), Size::zero()),
		};
		let mut unbreakable = default_unbreakable.clone();
		let mut font = lock(&self.font).unwrap();

		let mut next;
		let mut iter = self.into_iter();
		let mut current = iter.next();
		while let Some((cfg, c1)) = current {
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
				unbreakable.glyphs.push(font.get(c1, c2, line_height, cfg));
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
	fn render(&mut self, app: &mut Application, path: &mut NodePath) -> Void {
		if let LengthPolicy::Chunks(_fs) = self.policy {
			// everything is fine
		} else {
			let mut h = self.spot.1.h;
			if h != self.previous_height {
				self.previous_height = h;
				if let Some(margin) = self.margin {
					h = h.checked_sub(margin.total_on(Axis::Vertical) as usize)?;
				}
				self.deploy(Some(h));
				app.should_recompute = true;
			}
		}
		if self.dirty {
			self.dirty = false;
			let spot = self.get_content_spot_at(self.spot)?;
			let (mut dst, pitch, _) = app.blit(&spot, path);
			let (_, size) = spot;
			let px_width = RGBA * size.w;
			for _ in 0..size.h {
				let (line_dst, dst_next) = dst.split_at_mut(px_width);
				line_dst.fill(0);
				dst = dst_next.get_mut(pitch..)?;
			}
		}
		// let (pos, size) = self.spot;
		// let start = (pos.x as usize + pos.y as usize * app.output.size.w) * 4;
		// let stop = start + (size.w * 4);
		// app.output.pixels.get_mut(start..stop)?.fill(255);
		// let start = start + (size.h * app.output.size.w * 4);
		// let stop = start + (size.w * 4);
		// app.output.pixels.get_mut(start..stop)?.fill(255);
		None
	}

	fn margin(&self) -> Option<Margin> {
		self.margin
	}

	fn attach(&mut self, app: &mut Application, path: &NodePath) -> Void {
		self.deploy(match self.policy {
			LengthPolicy::Chunks(l) => Some(l),
			_ => None,
		});
		app.should_recompute = true;
		app.blit_hooks.push((path.clone(), self.get_spot()));
		None
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
		self.policy
	}

	fn children(&self) -> &[RcNode] {
		&self.children
	}

	fn get_spot(&self) -> Spot {
		self.spot
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.dirty = true;
		self.spot = spot;
		None
	}
}

/// This function is to be used in [`crate::xml::TreeParser::with`].
#[cfg(feature = "xml")]
pub fn paragraph(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	let mut text = Err(String::from("missing txt attribute"));
	let mut font_size = app.default_font_size;
	let mut font = None;
	let mut margin = None;

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"margin" => {
				let m = value.parse().map_err(|_| format!("bad value: {}", value))?;
				margin = Some(Margin {
					left: m,
					top: m,
					bottom: m,
					right: m,
				});
			},
			"txt" => text = Ok(value.clone()),
			"font" => font = Some(value.clone()),
			"font-size" => font_size = value.parse().ok().ok_or(format!("bad font-size: {}", &value))?,
			_ => unexpected_attr(&name)?,
		}
	}

	let err_msg = format!("unknown font: \"{}\"", font.as_ref().unwrap_or(&format!("<none>")));
	let font = app.fonts.get(&font).ok_or(err_msg)?.clone();

	let font_config = FontConfig {
		weight: 0,
		italic_angle: 0,
		underline: 0,
		overline: 0,
		opacity: 0,
		serif_rise: 0,
	};

	let policy = {
		let err_msg = format!("paragraph must be in a container");
		let parent = app.get_node(path).ok_or(err_msg.clone())?;
		let parent = parent.lock().unwrap();
		let (parent_axis, _) = parent.container().ok_or(err_msg)?;

		match parent_axis {
			Axis::Vertical => LengthPolicy::Chunks(font_size),
			Axis::Horizontal => LengthPolicy::WrapContent,
		}
	};

	let paragraph = rc_node(Paragraph {
		parts: {
			let mut vec = Vec::new();
			vec.push((font_config, text?));
			vec
		},
		font,
		children: Vec::new(),
		space_width: 10,
		policy,
		margin,
		spot: (Point::zero(), Size::zero()),
		previous_height: 0,
		dirty: true,
	});

	app.add_node(path, paragraph)?;

	Ok(())
}
