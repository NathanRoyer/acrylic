use railway::Program;
use railway::Couple;
use railway::ParsingError;

use crate::Size;
use crate::Point;
use crate::tree::Tree;
use crate::node::NodeKey;
use crate::node::PixelSource;

use std::collections::HashMap;

type Void = Option<()>;

#[allow(unused)]
pub struct Railway {
	program: Program,
	stack: Vec<Couple>,
}

impl Railway {
	pub fn new(bytes: &[u8]) -> Result<Self, ParsingError> {
		let program = Program::parse(bytes)?;
		let stack = program.create_stack();
		Ok(Self {
			program,
			stack,
		})
	}
}

pub const RGBA: usize = 4;
pub const RGB: usize = 3;
pub const BW: usize = 1;

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

pub struct Renderer {
	pub bmp_store: HashMap<(usize, usize), Bitmap>,
	pub rwy_store: HashMap<(usize, usize), Railway>,
	mask: Vec<u8>,
	output: Bitmap,
}

impl Renderer {
	pub fn new() -> Self {
		Self {
			bmp_store: HashMap::new(),
			rwy_store: HashMap::new(),
			mask: Vec::new(),
			output: Bitmap::new(Size::zero(), RGBA),
		}
	}

	pub fn get_output(&self) -> &Bitmap {
		&self.output
	}

	pub fn render(&mut self, t: &Tree, node: NodeKey) -> Void {
		let size = t.get_node_size(node)?;
		if size != self.output.size {
			self.output = Bitmap::new(size, RGBA);
			self.mask = vec![0; size.w * size.h];
		} else {
			self.output.pixels.fill(0);
		}
		self.render_cont(t, node)
	}

	fn render_cont(&mut self, t: &Tree, node: NodeKey) -> Void {
		for i in t.children(node) {
			self.render_cont(t, i);
		}
		self.render_node(t, node)
	}

	fn render_node(&mut self, t: &Tree, node: NodeKey) -> Void {
		let position = t.get_node_position(node)?;
		let size = t.get_node_size(node)?;
		let source = t.get_node_pixel_source(node)?;
		match source {
			PixelSource::Bitmap(i, j) => self.render_bitmap(position, size, (i, j)),
			PixelSource::Railway(i, j) => self.render_railway(position, size, (i, j)),
		}
	}

	fn render_bitmap(&mut self, mut position: Point, mut size: Size, i: (usize, usize)) -> Void {
		let img = self.bmp_store.get(&i)?;
		{
			let x = position.x as isize;
			let y = position.y as isize;
			let w = size.w as isize;
			let h = size.h as isize;
			position.x = x + img.margin.left;
			position.y = y + img.margin.top;
			size.w = (w - img.margin.total_h()).try_into().expect("render.rs: bad H margin!");
			size.h = (h - img.margin.total_v()).try_into().expect("render.rs: bad V margin!");
		}
		let spot_factor = (size.w - 1) as f32;
		let img_factor = (img.size.w - 1) as f32;
		let ratio = img_factor / spot_factor;
		let output_x = 0..self.output.size.w as isize;
		let output_y = 0..self.output.size.h as isize;
		for x in 0..size.w {
			for y in 0..size.h {
				let (ox, oy) = (position.x + x as isize, position.y + y as isize);
				if output_x.contains(&ox) && output_y.contains(&oy) {
					let (ox, oy) = (ox as usize, oy as usize);
					let i = (oy * self.output.size.w + ox) * RGBA;
					let x = ((x as f32) * ratio).round() as usize;
					let y = ((y as f32) * ratio).round() as usize;
					let j = (y * img.size.w + x) * RGBA;
					if let Some(src) = img.pixels.get(j..(j + RGBA)) {
						if let Some(dst) = self.output.pixels.get_mut(i..(i + RGBA)) {
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

	fn render_railway(&mut self, _position: Point, _size: Size, _i: (usize, usize)) -> Void {
		None
	}
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
