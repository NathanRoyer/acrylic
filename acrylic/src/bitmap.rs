use crate::Size;
use crate::Point;
use crate::Spot;
use crate::Void;
use crate::app::Application;
use crate::app::for_each_line;
use crate::node::Axis::Vertical;
use crate::node::Axis::Horizontal;
use crate::node::Node;
use crate::node::Margin;
use crate::node::NodePath;
use crate::node::LengthPolicy;
use crate::geometry::aspect_ratio;

use core::fmt::Debug;
use core::fmt::Result;
use core::fmt::Formatter;
use core::any::Any;

use std::string::String;
use std::vec::Vec;
use std::prelude::v1::vec;

pub type Channels = usize;

/// red, green, blue, alpha
pub const RGBA: Channels = 4;

/// red, green, blue
pub const RGB:  Channels = 3;

/// black and white
pub const BW:   Channels = 1;

/// This structure has two purposes.
///
/// First, it is used across this crate as a way to
/// store 2D pixel arrays.
///
/// It also implements [`Widget`], so it can be set to
/// a node so that this node renders as an image.
#[derive(Clone)]
pub struct Bitmap {
	/// The pixel array
	pub pixels: Vec<u8>,
	/// A resized copy cached for faster rendering
	pub cache: Vec<u8>,
	/// The number of channels; must be one of
	/// BW, RGB or RGBA.
	pub channels: Channels,
	/// The size (width and height) of the image
	pub size: Size,
	/// The screen spot of the node
	pub spot: Spot,
	pub margin: Option<Margin>,
	pub ratio: f64,
	pub dirty: bool,
}

impl Debug for Bitmap {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		f.debug_struct("Bitmap")
			.field("channels", &self.channels)
			.field("size", &self.size)
			.field("spot", &self.spot)
			.field("margin", &self.margin)
			.field("ratio", &self.ratio)
			.finish()
	}
}

const TRANSPARENT_PIXEL: [u8; 4] = [0; 4];

impl Bitmap {
	pub fn new(size: Size, channels: Channels, margin: Option<Margin>) -> Self {
		Self {
			size,
			channels,
			pixels: vec![0; channels * size.w * size.h],
			cache: Vec::new(),
			spot: (Point::zero(), Size::zero()),
			margin,
			dirty: true,
			ratio: {
				let (add_w, add_h) = match margin {
					Some(m) => (m.total_on(Horizontal), m.total_on(Vertical)),
					None => (0, 0),
				};
				aspect_ratio((size.w as isize + add_w) as usize, (size.h as isize + add_h) as usize)
			},
		}
	}

	pub fn update_cache(&mut self, spot: Spot, owned: bool) -> Void {
		assert!(self.channels == RGBA);
		let (_, size) = spot;
		let len = size.w * size.h * RGBA;
		if len != 0 && len != self.cache.len() {
			self.cache.resize(len, 0);
			let spot_factor = (size.w - 1) as f64;
			let img_factor = (self.size.w - 1) as f64;
			let ratio = img_factor / spot_factor;
			for y in 0..size.h {
				for x in 0..size.w {
					let i = (y * size.w + x) * RGBA;
					let x = round((x as f64) * ratio);
					let y = round((y as f64) * ratio);
					let j = (y * self.size.w + x) * RGBA;
					let src = self.pixels.get(j..(j + RGBA)).unwrap_or(&TRANSPARENT_PIXEL);
					let dst = self.cache.get_mut(i..(i + RGBA)).unwrap();
					if owned {
						dst.copy_from_slice(src);
					} else {
						// premultiplied alpha
						let a = src[3] as u32;
						for i in 0..3 {
							dst[i] = ((src[i] as u32 * a) / 255) as u8;
						}
						dst[3] = a as u8;
					}
				}
			}
		}
		Some(())
	}

	pub fn render_at(&mut self, app: &mut Application, path: &NodePath, spot: Spot) -> Void {
		let spot = self.get_content_spot_at(spot)?;
		let (dst, pitch, owned) = app.blit(&spot, Some(path))?;
		self.update_cache(spot, owned)?;
		let (_, size) = spot;
		let px_width = RGBA * size.w;
		let mut src = self.cache.chunks(px_width);
		for_each_line(dst, size, pitch, |_, line_dst| {
			let line_src = src.next().unwrap();
			if owned {
				line_dst.copy_from_slice(line_src);
			} else {
				let mut i = px_width as isize - 1;
				let mut a = 0;
				while i >= 0 {
					let j = i as usize;
					let (dst, src) = (&mut line_dst[j], &(line_src[j] as u32));
					if (j & 0b11) == 3 {
						a = (255 - *src) as u32;
					}
					*dst = (*src + (((*dst as u32) * a) / 255)) as u8;
					i -= 1;
				}
			}
		});
		Some(())
	}
}

impl Node for Bitmap {
	fn render(&mut self, app: &mut Application, path: &mut NodePath, _: usize) -> Option<usize> {
		if self.dirty {
			self.render_at(app, path, self.spot)?;
			self.dirty = false;
		}
		Some(0)
	}

	fn policy(&self) -> LengthPolicy {
		LengthPolicy::AspectRatio(self.ratio)
	}

	fn set_dirty(&mut self) {
		self.dirty = true;
	}

	fn margin(&self) -> Option<Margin> {
		self.margin
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
		String::from("Image")
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

#[cfg(feature = "std")]
#[inline(always)]
fn round(float: f64) -> usize {
	float.round() as usize
}

#[cfg(not(feature = "std"))]
#[inline(always)]
fn round(mut float: f64) -> usize {
	// given float > 0
	let integer = float as usize;
	float -= integer as f64;
	match float > 0.5 {
		true => integer + 1,
		false => integer,
	}
}
