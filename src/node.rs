use crate::Point;
use crate::Size;

use crate::tree::Tree;
use crate::tree::Command;
use crate::tree::CommandVariant;

pub type Hash = u64;

#[derive(Debug, Copy, Clone)]
pub enum LengthPolicy {
	Fixed(usize),
	Available(f64),
	Chunks(usize),
	WrapContent(u32, u32),
	AspectRatio(f64),
}

pub type BitmapOffset = Point;
pub type BitmapCrop = Size;
pub type BitmapMask = [f64; 4];

#[derive(Debug, Clone)]
pub enum PixelSource {
	Bitmap(usize, usize),
	Railway(usize, usize),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Axis {
	Horizontal,
	Vertical,
}

pub type NodeKey = usize;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Event {
	QuickAction1,
	QuickAction2,
	QuickAction3,
	QuickAction4,
	QuickAction5,
	QuickAction6,
	Modifier1,
	Modifier2,
	Factor1,
	Factor2,
	Pan1,
	Pan2,
	WheelX,
	WheelY,
	Resize,
	Delete,
	Drop,
	Load,
	WillOutput,
}

macro_rules! getter {
	($n:ident, $r:ty, $p:pat_param, $s:expr) => {
		pub fn $n(&self, i: NodeKey) -> Option<$r> {
			let mut retval = None;
			for i in self.range(i) {
				if let $p = self.nodes[i] {
					retval = Some($s);
				}
			}
			retval
		}
	}
}

/// Getters
impl Tree {
	getter!(get_node_position, Point, Command::Position(x, y), Point::new(x, y));

	getter!(get_node_size, Size, Command::Size(w, h), Size::new(w, h));

	getter!(get_node_policy, LengthPolicy, Command::LengthPolicy(policy), policy);

	getter!(get_node_name, Hash, Command::Name(hash), hash);

	getter!(get_node_container, Axis, Command::ContainerNode(axis), axis);

	getter!(get_node_bitmap_offset, BitmapOffset, Command::BitmapOffset(x, y), BitmapOffset::new(x, y));

	getter!(get_node_bitmap_crop, BitmapCrop, Command::BitmapCrop(w, h), BitmapCrop::new(w, h));

	pub fn get_node_pixel_source(&self, node: NodeKey) -> Option<PixelSource> {
		let mut retval = None;
		for i in self.range(node) {
			match self.nodes[i] {
				Command::BitmapSource(j1, j2)  => retval = Some(PixelSource::Bitmap(j1, j2)),
				Command::RailwaySource(j1, j2) => retval = Some(PixelSource::Railway(j1, j2)),
				_ => (),
			}
		}
		retval
	}

	pub fn get_node_bitmap_mask(&self, node: NodeKey) -> Option<BitmapMask> {
		let (mut rg, mut ba) = (None, None);
		for i in self.range(node) {
			if let Command::BitmapMaskRg(r, g) = self.nodes[i] {
				rg = Some((r, g));
			}
			if let Command::BitmapMaskBa(b, a) = self.nodes[i] {
				ba = Some((b, a));
			}
		}
		match (rg, ba) {
			(Some((r, g)), Some((b, a))) => Some([r, g, b, a]),
			_ => None,
		}
	}

	pub fn get_node_railway_parameters(&self, node: NodeKey) -> Vec<(usize, f32, f32)> {
		let mut retval = Vec::with_capacity(16);
		for i in self.range(node) {
			if let Command::RailwayParameter(j, x, y) = self.nodes[i] {
				retval.push((j, x, y));
			}
		}
		retval
	}

	pub fn get_node_handlers(&self, node: NodeKey, event: Event) -> Vec<Hash> {
		let mut retval = Vec::with_capacity(8);
		for i in self.range(node) {
			if let Command::Handler(e, hash) = self.nodes[i] {
				if e == event {
					retval.push(hash);
				}
			}
		}
		retval
	}
}

macro_rules! setter {
	($n:ident, $b:expr, $t:ty, $i:ident, $c:expr, $v:expr) => {
		pub fn $n(&mut self, i: &mut NodeKey, cmd: Option<$t>) {
			if let Some($i) = cmd {
				self.add_command(i, $c, $b);
			} else {
				self.del_variant(*i, $v);
			}
		}
	}
}

impl Tree {
	setter!(set_node_position, true, Point, p, Command::Position(p.x, p.y), CommandVariant::Position);

	setter!(set_node_size, true, Size, s, Command::Size(s.w, s.h), CommandVariant::Size);

	setter!(set_node_policy, true, LengthPolicy, p, Command::LengthPolicy(p), CommandVariant::LengthPolicy);

	setter!(set_node_name, true, Hash, n, Command::Name(n), CommandVariant::Name);

	setter!(set_node_container, true, Axis, a, Command::ContainerNode(a), CommandVariant::ContainerNode);

	setter!(set_node_bitmap_offset, true, BitmapOffset, o, Command::BitmapOffset(o.x, o.y), CommandVariant::BitmapOffset);

	setter!(set_node_bitmap_crop, true, BitmapCrop, c, Command::BitmapCrop(c.w, c.h), CommandVariant::BitmapCrop);

	setter!(set_node_template, true, NodeKey, t, Command::Template(t), CommandVariant::Template);

	pub fn set_node_pixel_source(&mut self, i: &mut NodeKey, cmd: Option<PixelSource>) {
		if let Some(src) = cmd {
			self.add_command(i, match src {
				PixelSource::Bitmap(j1, j2)  => Command::BitmapSource(j1, j2),
				PixelSource::Railway(j1, j2) => Command::RailwaySource(j1, j2),
			}, true);
		} else {
			self.del_variant(*i, CommandVariant::BitmapSource);
			self.del_variant(*i, CommandVariant::RailwaySource);
		}
	}

	pub fn set_node_bitmap_mask(&mut self, i: &mut NodeKey, cmd: Option<BitmapMask>) {
		if let Some([r, g, b, a]) = cmd {
			self.add_command(i, Command::BitmapMaskRg(r, g), true);
			self.add_command(i, Command::BitmapMaskBa(b, a), true);
		} else {
			self.del_variant(*i, CommandVariant::BitmapMaskRg);
			self.del_variant(*i, CommandVariant::BitmapMaskBa);
		}
	}

	pub fn set_node_railway_parameter(&mut self, node: &mut NodeKey, p: usize, value: Option<(f32, f32)>) {
		if let Some((x, y)) = value {
			// add_command will use cmd1.replaceable(cmd2)
			self.add_command(node, Command::RailwayParameter(p, x, y), true);
		} else {
			for i in self.range(*node) {
				if let Command::RailwayParameter(j, _, _) = self.nodes[i] {
					if j == p {
						self.skip_command(*node, i);
					}
				}
			}
		}
	}

	pub fn set_node_handlers(&mut self, node: &mut NodeKey, event: Event, names: Vec<Hash>) {
		for i in self.range(*node) {
			if let Command::Handler(e, _) = self.nodes[i] {
				if e == event {
					self.skip_command(*node, i);
				}
			}
		}
		for name in names {
			self.add_command(node, Command::Handler(event, name), false);
		}
	}
}
