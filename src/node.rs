use bitflags::bitflags;

use crate::Point;
use crate::Size;

use crate::tree::Tree;
use crate::tree::Command;
use crate::tree::CommandVariant;
use crate::application::RcWidget;

#[derive(Debug, Copy, Clone)]
pub enum LengthPolicy {
	Fixed(usize),
	Available(f64),
	Chunks(usize),
	WrapContent(u32, u32),
	AspectRatio(f64),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Axis {
	Horizontal,
	Vertical,
}

pub type NodeKey = usize;
pub type Hash = u64;

bitflags! {
	pub struct EventFlags: u32 {
		const QUICK_ACTION_1 = 0b00000000000000001;
		const QUICK_ACTION_2 = 0b00000000000000010;
		const QUICK_ACTION_3 = 0b00000000000000100;
		const QUICK_ACTION_4 = 0b00000000000001000;
		const QUICK_ACTION_5 = 0b00000000000010000;
		const QUICK_ACTION_6 = 0b00000000000100000;
		const MODIFIER_1     = 0b00000000001000000;
		const MODIFIER_2     = 0b00000000010000000;
		const FACTOR_1       = 0b00000000100000000;
		const FACTOR_2       = 0b00000001000000000;
		const PAN_1          = 0b00000010000000000;
		const PAN_2          = 0b00000100000000000;
		const WHEEL_X        = 0b00001000000000000;
		const WHEEL_Y        = 0b00010000000000000;
		const RESIZE         = 0b00100000000000000;
		const DELETE         = 0b01000000000000000;
		const LOAD           = 0b10000000000000000;
	}
}

macro_rules! getter {
	($n:ident, $r:ty, $p:pat_param, $s:expr) => {
		pub fn $n(&self, i: NodeKey) -> Option<$r> {
			let mut retval = None;
			for i in self.range(i) {
				if let $p = &self.nodes[i] {
					retval = Some($s);
				}
			}
			retval
		}
	}
}

/// Getters
impl Tree {
	getter!(get_node_spot, (Point, Size), Command::Spot(x, y, w, h), (Point::new(*x as isize, *y as isize), Size::new(*w as usize, *h as usize)));

	getter!(get_node_policy, LengthPolicy, Command::LengthPolicy(policy), *policy);

	getter!(get_node_name, Hash, Command::Name(hash), *hash);

	getter!(get_node_container, Axis, Command::ContainerNode(axis), *axis);

	getter!(get_node_widget, RcWidget, Command::Widget(a), a.clone());

	getter!(get_node_handler, EventFlags, Command::Handler(m), *m);
}

macro_rules! setter {
	($n:ident, $b:expr, $t:ty, $i:pat_param, $c:expr, $v:expr) => {
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
	setter!(set_node_spot, true, (Point, Size), (p, s), Command::Spot(p.x as i32, p.y as i32, s.w as u32, s.h as u32), CommandVariant::Spot);

	setter!(set_node_policy, true, LengthPolicy, p, Command::LengthPolicy(p), CommandVariant::LengthPolicy);

	setter!(set_node_name, true, Hash, n, Command::Name(n), CommandVariant::Name);

	setter!(set_node_container, true, Axis, a, Command::ContainerNode(a), CommandVariant::ContainerNode);

	setter!(set_node_template, true, NodeKey, t, Command::Template(t), CommandVariant::Template);

	setter!(set_node_widget, true, RcWidget, a, Command::Widget(a), CommandVariant::Widget);

	setter!(set_node_handler, true, EventFlags, a, Command::Handler(a), CommandVariant::Handler);
}
