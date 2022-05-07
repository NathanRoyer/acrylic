use crate::Size;
use crate::Void;
use crate::tree::NodeKey;
use crate::app::Application;
use crate::app::Widget;

use core::fmt::Debug;
use core::any::Any;

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
}

impl Bitmap {
	pub fn new(size: Size, channels: Channels) -> Self {
		Self {
			size,
			channels,
			pixels: vec![0; channels * size.w * size.h],
		}
	}
}

impl Widget for Bitmap {
	fn render(&mut self, app: &mut Application, node: NodeKey) -> Void {
		assert!(self.channels == RGBA);
		let (mut position, mut size) = app.tree.get_node_spot(node)?;
		let dst = &mut app.output;
		assert!(dst.channels == RGBA);
		if let Some(margin) = app.tree.get_node_margin(node) {
			let x = position.x as isize;
			let y = position.y as isize;
			let w = size.w as isize;
			let h = size.h as isize;
			position.x = x + margin.left;
			position.y = y + margin.top;
			size.w = (w - margin.total_h()).try_into().expect("render.rs: bad H margin!");
			size.h = (h - margin.total_v()).try_into().expect("render.rs: bad V margin!");
		}
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
						let x = ((x as f32) * ratio).round() as usize;
						let y = ((y as f32) * ratio).round() as usize;
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

	fn legend(&mut self, _: &mut Application, _: NodeKey) -> String {
		String::from("Image")
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}
