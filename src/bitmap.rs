use crate::Size;
use crate::Void;
use crate::node::NodeKey;
use crate::application::Application;
use crate::application::Widget;

use std::fmt::Debug;
use std::any::Any;

pub const RGBA: usize = 4;
pub const RGB: usize = 3;
pub const BW: usize = 1;

#[derive(Debug, Clone)]
pub struct Bitmap {
	pub pixels: Vec<u8>,
	pub channels: usize,
	pub size: Size,
	pub margin: Margin,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Margin {
	pub top: isize,
	pub bottom: isize,
	pub left: isize,
	pub right: isize,
}

impl Bitmap {
	pub fn new(size: Size, channels: usize) -> Self {
		Self {
			margin: Margin::zero(),
			size,
			channels,
			pixels: vec![0; channels * size.w * size.h],
		}
	}

	pub fn size(&self) -> Size {
		let w = self.size.w as isize + self.margin.total_h();
		let h = self.size.h as isize + self.margin.total_v();
		Size {
			w: w as usize,
			h: h as usize,
		}
	}
}

impl Widget for Bitmap {
	fn render(&mut self, app: &mut Application, node: NodeKey) -> Void {
		assert!(self.channels == RGBA);
		let (mut position, mut size) = app.tree.get_node_spot(node)?;
		let dst = &mut app.output;
		assert!(dst.channels == RGBA);
		{
			let x = position.x as isize;
			let y = position.y as isize;
			let w = size.w as isize;
			let h = size.h as isize;
			position.x = x + self.margin.left;
			position.y = y + self.margin.top;
			size.w = (w - self.margin.total_h()).try_into().expect("render.rs: bad H margin!");
			size.h = (h - self.margin.total_v()).try_into().expect("render.rs: bad V margin!");
		}
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
		None
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

impl Margin {
	pub fn zero() -> Self {
		Self {
			top: 0,
			bottom: 0,
			left: 0,
			right: 0,
		}
	}

	pub fn total_v(&self) -> isize {
		self.top + self.bottom
	}

	pub fn total_h(&self) -> isize {
		self.left + self.right
	}
}
