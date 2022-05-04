use bitflags::bitflags;

use crate::Point;
use crate::Size;
use crate::Void;
use crate::application::RcWidget;
use crate::flexbox::compute_tree;

use core::fmt::Display;
use core::fmt::Formatter;
use core::fmt::Result;
use core::cmp::Ordering;
use core::ops::Range;
use core::mem::swap;
use core::mem::size_of;

const SKIP_CONTINUED: usize = 0;
const COMMAND_SIZE_IN_BYTES: usize = 24;

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Margin {
	pub top: isize,
	pub bottom: isize,
	pub left: isize,
	pub right: isize,
}

pub type NodeKey = usize;
pub type Hash = u64;

bitflags! {
	pub struct EventFlags: u32 {
		const QUICK_ACTION_1 = 0b0000000000000001;
		const QUICK_ACTION_2 = 0b0000000000000010;
		const QUICK_ACTION_3 = 0b0000000000000100;
		const QUICK_ACTION_4 = 0b0000000000001000;
		const QUICK_ACTION_5 = 0b0000000000010000;
		const QUICK_ACTION_6 = 0b0000000000100000;
		const MODIFIER_1     = 0b0000000001000000;
		const MODIFIER_2     = 0b0000000010000000;
		const FACTOR_1       = 0b0000000100000000;
		const FACTOR_2       = 0b0000001000000000;
		const PAN_1          = 0b0000010000000000;
		const PAN_2          = 0b0000100000000000;
		const WHEEL_X        = 0b0001000000000000;
		const WHEEL_Y        = 0b0010000000000000;
		const DELETE         = 0b0100000000000000;
	}
}

#[derive(Debug, Copy, Clone)]
pub enum Event {
	QuickAction1,
	QuickAction2,
	QuickAction3,
	QuickAction4,
	QuickAction5,
	QuickAction6,
	Modifier1(bool),
	Modifier2(bool),
	Factor1(f64),
	Factor2(f64),
	Pan1(usize, usize),
	Pan2(usize, usize),
	WheelX(f64),
	WheelY(f64),
	Delete,
}

#[derive(Debug, Clone)]
pub(crate) enum Command {
	Skip(usize),
	Node(NodeKey, usize),
	Child(NodeKey),
	Template(NodeKey),

	Spot(i32, i32, u32, u32),
	Margin(i32, i32, i32, i32),
	LengthPolicy(LengthPolicy),
	Name(Hash),
	Handler(EventFlags),
	ContainerNode(Axis),
	Widget(RcWidget),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CommandVariant {
	Skip,
	Node,
	Child,
	Template,

	Spot,
	Margin,
	LengthPolicy,
	Name,
	Handler,
	ContainerNode,
	Widget,
}

#[derive(Debug, Clone)]
pub struct Tree {
	pub(crate) nodes: Vec<Command>,
}

impl Tree {
	pub fn new() -> Self {
		if size_of::<Command>() != COMMAND_SIZE_IN_BYTES {
			println!("Warning! (acrylic/common/tree.rs): Command has an unpredicted size");
		}
		Self {
			nodes: Vec::new(),
		}
	}

	fn next_skip(&mut self, key: NodeKey) -> Option<NodeKey> {
		let mut i = key + self.length(key);
		let mut result = None;
		let max = self.nodes.len();
		while i < max {
			if let Command::Skip(l) = self.nodes[i] {
				if l != SKIP_CONTINUED {
					if let Some(j) = result {
						let distance = i - j;
						self.nodes[j] = Command::Skip(distance + l);
						self.nodes[i] = Command::Skip(SKIP_CONTINUED);
					} else {
						result = Some(i);
					}
				}
				i += l;
			} else {
				break;
			}
		}
		result
	}

	// caller must always fill the returned slot
	// properly, discarding its content
	fn find_slot(&mut self, required: usize) -> NodeKey {
		let mut empty = 0;
		let mut first_of_skip_sequence = 0;
		let mut i = 0;
		while i < self.nodes.len() {
			i += match self.nodes[i] {
				Command::Skip(l) => {
					if empty == 0 {
						first_of_skip_sequence = i;
					}
					empty += l;
					if empty >= required {
						if empty > required {
							let excess = empty - required;
							self.nodes[i + l - excess] = Command::Skip(excess);
							// the rest of the commands are skips already
						}
						return first_of_skip_sequence;
					}
					l
				},
				Command::Node(_, l) => (empty = 0, l).1,
				_ => unreachable!(),
			};
		}
		// we're here = not enough space
		// append skips to get a big-enough slot
		i = self.nodes.len() - empty;
		let new_len = i + required;
		self.nodes.resize(new_len, Command::Skip(0));
		i
	}

	pub fn add_node(&mut self, parent: Option<&mut NodeKey>, add_skips: usize) -> NodeKey {
		let required = 1 + add_skips;
		let i = self.find_slot(required);
		self.nodes[i] = Command::Node(match parent {
			Some(ref p) => **p,
			None => usize::MAX,
		}, 1);
		self.nodes[i..][..required][1..].fill(Command::Skip(1));
		if let Some(p) = parent {
			self.add_command(p, Command::Child(i), false);
		}
		i
	}

	fn append_command(&mut self, node: NodeKey, cmd: &mut Command) -> Option<()> {
		let i = self.next_skip(node)?;
		match self.nodes[i] {
			Command::Skip(l) if l == 1 => {
				swap(&mut self.nodes[i], cmd);
			},
			Command::Skip(l) if l > 1 => {
				swap(&mut self.nodes[i], cmd);
				self.nodes[i + 1] = Command::Skip(l - 1);
			},
			_ => unreachable!(),
		}
		if let Command::Node(p, l) = self.nodes[node] {
			self.nodes[node] = Command::Node(p, l + 1);
		}
		Some(())
	}

	fn subslice(&mut self, node: NodeKey) -> &mut [Command] {
		let r = self.range(node);
		&mut self.nodes[r]
	}

	fn populate_skip(subslice: &mut [Command]) {
		let length = subslice.len();
		subslice[0] = Command::Skip(length);
		if length > 1 {
			subslice[1..].fill(Command::Skip(SKIP_CONTINUED));
		}
	}

	pub fn del_node(&mut self, node: NodeKey, recursive: bool) {
		if let Some(p) = self.parent(node) {
			let p_range = self.range(p);
			for i in p_range.start..p_range.end {
				match self.nodes[i] {
					Command::Child(c) if c == node => {
						let last = p_range.end - 1;
						self.nodes.swap(i, last);
						self.nodes[last] = Command::Skip(1);
					},
					_ => (),
				}
			}
		}
		if recursive {
			for k in self.children(node) {
				self.del_node(k, true);
			}
		}
		Self::populate_skip(self.subslice(node));
	}

	fn pull(&mut self, node: NodeKey) -> Vec<Command> {
		let subslice = self.subslice(node);
		let commands = subslice.to_vec();
		Self::populate_skip(subslice);
		commands
	}

	fn update_relatives(&mut self, keys: (NodeKey, NodeKey)) {
		if let Some(p) = self.parent(keys.1) {
			for i in self.range(p) {
				match self.nodes[i] {
					Command::Child(c) if c == keys.0 => {
						self.nodes[i] = Command::Child(keys.1);
					},
					_ => (),
				}
			}
		}

		for c in self.children(keys.1) {
			if let Command::Node(_, l) = self.nodes[c] {
				self.nodes[c] = Command::Node(keys.1, l);
			}
		}
	}

	pub(crate) fn skip_command(&mut self, node: NodeKey, i: usize) {
		let (parent, length) = self.parent_and_length(node);
		let end = node + length;
		let last = end - 1;
		self.nodes.swap(i, last);
		self.nodes[node] = Command::Node(parent, length - 1);
		self.nodes[last] = Command::Skip(1);
	}

	pub(crate) fn add_command(&mut self, node: &mut NodeKey, mut cmd: Command, replace: bool) {
		if replace {
			for i in self.range(*node) {
				if self.nodes[i].variant() == cmd.variant() {
					swap(&mut self.nodes[i], &mut cmd);
					return;
				}
			}
		}
		if let None = self.append_command(*node, &mut cmd) {
			let (parent, length) = self.parent_and_length(*node);
			let length = length + 1;
			let mut commands = self.pull(*node);
			commands.push(cmd);
			commands[0] = Command::Node(parent, length);
			let slot = self.find_slot(length);
			self.nodes[slot..][..length].swap_with_slice(&mut commands);
			self.update_relatives((*node, slot));
			*node = slot;
		}
	}

	pub(crate) fn del_variant(&mut self, node: NodeKey, variant: CommandVariant) {
		for i in self.range(node) {
			if self.nodes[i].variant() == variant {
				self.skip_command(node, i);
			}
		}
	}

	fn length(&self, node: NodeKey) -> usize {
		match self.nodes[node] {
			Command::Skip(l) => l,
			Command::Node(_, l) => l,
			_ => unreachable!(),
		}
	}

	pub(crate) fn range(&self, node: NodeKey) -> Range<usize> {
		node..(node + self.length(node))
	}

	fn parent_and_length(&self, node: NodeKey) -> (NodeKey, usize) {
		match self.nodes[node] {
			Command::Node(p, l) => (p, l),
			_ => unreachable!(),
		}
	}

	pub fn parent(&self, node: NodeKey) -> Option<NodeKey> {
		let p = self.parent_and_length(node).0;
		match p == usize::MAX {
			true => None,
			false => Some(p),
		}
	}

	pub fn get_node_root(&self, mut node: NodeKey) -> NodeKey {
		loop {
			match self.parent(node) {
				Some(p) => node = p,
				None => break node,
			};
		}
	}

	pub fn children(&self, node: NodeKey) -> Vec<NodeKey> {
		let mut result = Vec::with_capacity(20);
		for i in self.range(node) {
			if let Command::Child(k) = self.nodes[i] {
				result.push(k);
			}
		}
		result
	}

	pub fn compute_flexbox(&mut self, root: NodeKey) {
		compute_tree(self, root);
	}

	/// in bytes
	pub fn memory_usage(&self) -> usize {
		self.nodes.len() * COMMAND_SIZE_IN_BYTES
	}

	fn show_rec(&self, k: NodeKey, d: usize) -> Void {
		let (position, size) = self.get_node_spot(k)?;
		println!("{}{}: {}x{} at {}x{}", "\t".repeat(d), k, size.w, size.h, position.x, position.y);
		for i in self.children(k) {
			self.show_rec(i, d + 1);
		}
		None
	}

	pub fn show(&self, k: NodeKey) {
		self.show_rec(k, 0);
	}
}

macro_rules! getter {
	($n:ident, $r:ty, $p:pat_param, $s:expr) => {
		pub fn $n(&self, mut i: NodeKey) -> Option<$r> {
			'outer: loop {
				for i in self.range(i) {
					if let $p = &self.nodes[i] {
						break 'outer Some($s);
					}
				}
				i = self.parent(i)?;
			}
		}
	}
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

/// Getters
impl Tree {
	getter!(get_node_spot, (Point, Size), Command::Spot(x, y, w, h), (Point::new(*x as isize, *y as isize), Size::new(*w as usize, *h as usize)));
	getter!(get_node_margin, Margin, Command::Margin(t, b, l, r), Margin::new(*t as isize, *b as isize, *l as isize, *r as isize));
	getter!(get_node_policy, LengthPolicy, Command::LengthPolicy(policy), *policy);
	getter!(get_node_name, Hash, Command::Name(hash), *hash);
	getter!(get_node_container, Axis, Command::ContainerNode(axis), *axis);
	getter!(get_node_widget, RcWidget, Command::Widget(a), a.clone());
	getter!(get_node_handler, EventFlags, Command::Handler(m), *m);
}

/// Setters
impl Tree {
	setter!(set_node_spot, true, (Point, Size), (p, s), Command::Spot(p.x as i32, p.y as i32, s.w as u32, s.h as u32), CommandVariant::Spot);
	setter!(set_node_margin, true, Margin, m, Command::Margin(m.top as i32, m.bottom as i32, m.left as i32, m.right as i32), CommandVariant::Margin);
	setter!(set_node_policy, true, LengthPolicy, p, Command::LengthPolicy(p), CommandVariant::LengthPolicy);
	setter!(set_node_name, true, Hash, n, Command::Name(n), CommandVariant::Name);
	setter!(set_node_container, true, Axis, a, Command::ContainerNode(a), CommandVariant::ContainerNode);
	setter!(set_node_template, true, NodeKey, t, Command::Template(t), CommandVariant::Template);
	setter!(set_node_widget, true, RcWidget, a, Command::Widget(a), CommandVariant::Widget);
	setter!(set_node_handler, true, EventFlags, a, Command::Handler(a), CommandVariant::Handler);
}

impl Margin {
	pub fn new(top: isize, bottom: isize, left: isize, right: isize) -> Self {
		Self {
			top,
			bottom,
			left,
			right,
		}
	}

	pub fn total_v(&self) -> isize {
		self.top + self.bottom
	}

	pub fn total_h(&self) -> isize {
		self.left + self.right
	}
}

impl Axis {
	pub fn is(self, other: Self) -> Void {
		if other == self {
			Some(())
		} else {
			None
		}
	}
}

impl Command {
	pub fn variant(&self) -> CommandVariant {
		match self {
			Command::Skip(_)                   => CommandVariant::Skip,
			Command::Node(_, _)                => CommandVariant::Node,
			Command::Child(_)                  => CommandVariant::Child,
			Command::Template(_)               => CommandVariant::Template,

			Command::Spot(_, _, _, _)          => CommandVariant::Spot,
			Command::Margin(_, _, _, _)        => CommandVariant::Margin,
			Command::LengthPolicy(_)           => CommandVariant::LengthPolicy,
			Command::Name(_)                   => CommandVariant::Name,
			Command::Handler(_)                => CommandVariant::Handler,
			Command::ContainerNode(_)          => CommandVariant::ContainerNode,
			Command::Widget(_)                 => CommandVariant::Widget,
		}
	}
}

impl Display for Tree {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		write!(f, "[\n")?;
		for i in self.nodes.chunks(8) {
			write!(f, "\t")?;
			for j in i {
				write!(f, "{}, ", j)?;
			}
			write!(f, "\n")?;
		}
		write!(f, "]")
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		let sym = match self {
			Command::Skip(_)                   => "__",
			Command::Node(_, l)                => {
				return write!(f, "\x1b[7m{:02x}\x1b[0m", l);
			},
			Command::Child(_)                  => "CH",
			Command::Template(_)               => "TM",

			Command::Spot(_, _, _, _)          => "SP",
			Command::Margin(_, _, _, _)        => "MA",
			Command::LengthPolicy(_)           => "LP",
			Command::Name(_)                   => "NM",
			Command::Handler(_)                => "HA",
			Command::ContainerNode(_)          => "CN",
			Command::Widget(_)                 => "WG",
		};
		write!(f, "{}", sym)
	}
}

impl Ord for Command {
	fn cmp(&self, other: &Self) -> Ordering {
		self.variant().cmp(&other.variant())
	}
}

impl PartialOrd for Command {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.variant().cmp(&other.variant()))
	}
}

impl Eq for Command {}

impl PartialEq for Command {
	fn eq(&self, other: &Self) -> bool {
		self.variant() == other.variant()
	}
}
