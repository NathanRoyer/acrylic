use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;
use std::cmp::Ordering;
use std::ops::Range;
use std::mem::swap;

use crate::node::LengthPolicy;
use crate::node::NodeKey;
use crate::node::EventFlags;
use crate::node::Axis;
use crate::node::Hash;
use crate::application::RcWidget;

const SKIP_CONTINUED: usize = 0;
const COMMAND_SIZE_IN_BYTES: usize = 24;

#[derive(Debug, Clone)]
pub(crate) enum Command {
	Skip(usize),
	Node(NodeKey, usize),
	Child(NodeKey),
	Template(NodeKey),

	Spot(i32, i32, u32, u32),
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
		if std::mem::size_of::<Command>() != COMMAND_SIZE_IN_BYTES {
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
				Command::Node(_, l) => {
					empty = 0;
					l
				},
				_ => {
					println!("{}; {}", i, self);
					unreachable!()
				},
			};
		}
		// we're here = not enough space
		// push garbage to get a big-enough slot
		i = self.nodes.len() - empty;
		let new_len = i + required;
		self.nodes.resize(new_len, Command::Skip(0));
		i
	}

	pub fn new_child(&mut self, parent: Option<&mut NodeKey>, add_skips: usize) -> NodeKey {
		let required = 1 + add_skips;
		let i = self.find_slot(required);
		self.nodes[i] = Command::Node(match parent {
			Some(ref p) => **p,
			None => 0,
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
		let p_range = self.range(self.parent(node));
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
		for i in self.range(self.parent(keys.1)) {
			match self.nodes[i] {
				Command::Child(c) if c == keys.0 => {
					self.nodes[i] = Command::Child(keys.1);
				},
				_ => (),
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

	pub fn parent(&self, node: NodeKey) -> NodeKey {
		self.parent_and_length(node).0
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

	/// in bytes
	pub fn memory_usage(&self) -> usize {
		self.nodes.len() * COMMAND_SIZE_IN_BYTES
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
			Command::LengthPolicy(_)           => "LP",
			Command::Name(_)                   => "NM",
			Command::Handler(_)                => "HA",
			Command::ContainerNode(_)          => "CN",
			Command::Widget(_)                 => "WG",
		};
		write!(f, "{}", sym)
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
			Command::LengthPolicy(_)           => CommandVariant::LengthPolicy,
			Command::Name(_)                   => CommandVariant::Name,
			Command::Handler(_)                => CommandVariant::Handler,
			Command::ContainerNode(_)          => CommandVariant::ContainerNode,
			Command::Widget(_)                 => CommandVariant::Widget,
		}
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
