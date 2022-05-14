use crate::Size;
use crate::Point;
use crate::Spot;
use crate::Void;
use crate::app::Application;
use crate::node::Node;
use crate::node::Margin;
use crate::node::NodePath;
use crate::node::LengthPolicy;
use crate::geometry::aspect_ratio;

use core::fmt::Debug;
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
#[derive(Debug, Clone)]
pub struct Bitmap {
	/// The pixel array
	pub pixels: Vec<u8>,
	/// The number of channels; must be one of
	/// BW, RGB or RGBA.
	pub channels: Channels,
	/// The size (width and height) of the image
	pub size: Size,
	/// The screen spot of the node
	pub spot: Spot,
	pub margin: Option<Margin>,
	pub ratio: f64,
}

impl Bitmap {
	pub fn new(size: Size, channels: Channels, margin: Option<Margin>) -> Self {
		Self {
			size,
			channels,
			pixels: vec![0; channels * size.w * size.h],
			spot: (Point::zero(), Size::zero()),
			margin,
			ratio: {
				let (add_w, add_h) = match margin {
					Some(m) => (m.total_h(), m.total_v()),
					None => (0, 0),
				};
				aspect_ratio((size.w as isize + add_w) as usize, (size.h as isize + add_h) as usize)
			},
		}
	}

	pub fn render_at(&mut self, app: &mut Application, spot: Spot) -> Void {
		assert!(self.channels == RGBA);
		let (position, size) = self.get_content_spot_at(spot)?;
		let dst = &mut app.output;
		assert!(dst.channels == RGBA);
		if size.w > 0 && size.h > 0 {
			let spot_factor = (size.w - 1) as f32;
			let img_factor = (self.size.w - 1) as f32;
			let ratio = img_factor / spot_factor;
			let dst_x = 0..dst.size.w as isize;
			let dst_y = 0..dst.size.h as isize;
			for x in 0..size.w {
				for y in 0..size.h {
					let (ox, oy) = (position.x + x as isize, position.y + y as isize);
					if dst_x.contains(&ox) && dst_y.contains(&oy) {
						let (ox, oy) = (ox as usize, oy as usize);
						let i = (oy * dst.size.w + ox) * RGBA;
						let x = round((x as f32) * ratio);
						let y = round((y as f32) * ratio);
						let j = (y * self.size.w + x) * RGBA;
						if let Some(src) = self.pixels.get(j..(j + RGBA)) {
							if let Some(dst) = dst.pixels.get_mut(i..(i + RGBA)) {
								for c in 0..RGBA {
									dst[c] = dst[c].checked_add(src[c]).unwrap_or(255);
								}
							}
						}
					}
				}
			}
		}
		None
	}
}

impl Node for Bitmap {
	fn render(&mut self, app: &mut Application, _path: &mut NodePath) -> Void {
		self.render_at(app, self.spot)
	}

	fn policy(&self) -> LengthPolicy {
		LengthPolicy::AspectRatio(self.ratio)
	}

	fn margin(&self) -> Option<Margin> {
		self.margin
	}

	fn get_spot(&self) -> Spot {
		self.spot
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
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
fn round(float: f32) -> usize {
	float.round() as usize
}

#[cfg(not(feature = "std"))]
#[inline(always)]
fn round(mut float: f32) -> usize {
	// given float > 0
	let integer = float as usize;
	float -= integer as f32;
	match float > 0.5 {
		true => integer + 1,
		false => integer,
	}
}
