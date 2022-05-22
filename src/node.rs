use bitflags::bitflags;

use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Void;
use crate::app::Application;
use crate::bitmap::RGBA;

#[cfg(feature = "railway")]
use railway::Program;
#[cfg(feature = "railway")]
use railway::Couple;
#[cfg(feature = "railway")]
use railway::RWY_PXF_RGBA8888;
#[cfg(feature = "railway")]
use lazy_static::lazy_static;

use core::any::Any;
use core::fmt::Debug;

use std::sync::Arc;
use std::sync::Mutex;
use std::string::String;
use std::vec::Vec;

/// This allows nodes to be layed out in various ways
/// by our flexbox-like algorithm. This structure
/// helps decide the main axis length; the cross axis
/// length depends on the container and cannot be
/// impacted by the children of the container.
#[derive(Debug, Copy, Clone)]
pub enum LengthPolicy {
	// needs two passes in diff-axis config
	// needs one pass in same-axis config
	/// Main length is just enough to contain all children.
	/// Valid for containers only.
	WrapContent,
	/// Main length is a fixed number of pixels
	Fixed(usize),
	/// Main length is divided in chunks of specified
	/// length (in pixels). The number of chunks is
	/// determined by the contained nodes: there will
	/// be as many chunks as necessary for all children
	/// to fit in.
	/// Valid for diff-axis [todo: explain diff-axis] containers only.
	Chunks(usize),
	/// Main length is computed from the cross length
	/// so that the size of the node maintains a certain
	/// aspect ratio.
	AspectRatio(f64),
	/// todo: doc
	Remaining(f64),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Axis {
	Horizontal,
	Vertical,
}

/// This can be used by [`crate::application::Widget`] implementations
/// to offset the boundaries of their original
/// rendering spot.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Margin {
	pub top: isize,
	pub bottom: isize,
	pub left: isize,
	pub right: isize,
}

bitflags! {
	pub struct EventMask: u32 {
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
		const TEXT_INPUT     = 0b0100000000000000;
		const DELETE         = 0b1000000000000000;
	}
}

/// An event which widgets can handle.
#[derive(Debug, Clone)]
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
	TextInput(String),
	Delete,
}

pub type NodePath = Vec<usize>;

pub trait Node: Debug + Any + 'static {
	/// `as_any` is required for as long as upcasting coercion is unstable
	fn as_any(&mut self) -> &mut dyn Any;

	#[allow(unused)]
	fn render(&mut self, app: &mut Application, path: &mut NodePath, style: usize) -> Option<usize> {
		None
	}

	/// The `handle` method is called when the platform forwards an event
	/// to the application. Using `app.tree`, one can manipulate the node
	/// identified by the `node` argument in reaction.
	///
	/// To receive events via this interface, you must first initialize
	/// the node using [`Tree::set_node_handler`].
	#[allow(unused)]
	fn handle(&mut self, app: &mut Application, path: &NodePath, event: Event) -> Void {
		None
	}

	/// Once you add [`DataRequest`]s to `app.data_requests`, the platform
	/// should fetch the data you requested. Once it has fetched the data,
	/// It will call the `loaded` method.
	#[allow(unused)]
	fn loaded(&mut self, app: &mut Application, path: &NodePath, name: &str, offset: usize, data: &[u8]) -> Void {
		None
	}

	#[allow(unused)]
	fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
		Ok(())
	}

	#[allow(unused)]
	fn add_node(&mut self, child: RcNode) -> Result<usize, String> {
		Err(String::from("Not a container"))
	}

	#[allow(unused)]
	fn replace_node(&mut self, index: usize, child: RcNode) -> Result<(), String> {
		Err(String::from("Not a container"))
	}

	fn margin(&self) -> Option<Margin> {
		None
	}

	fn policy(&self) -> LengthPolicy {
		LengthPolicy::Fixed(0)
	}

	fn set_dirty(&mut self) {
		// do nothing by default
	}

	/// The `describe` method is called when the platform needs a
	/// textual description of a node. This helps making
	/// applications accessible to people with disabilities.
	fn describe(&self) -> String;

	#[allow(unused)]
	fn children(&self) -> &[RcNode] {
		&[]
	}

	fn get_spot(&self) -> Spot {
		(Point::zero(), Size::zero())
	}

	fn get_content_spot_at(&self, mut spot: Spot) -> Option<Spot> {
		if let Some(margin) = self.margin() {
			spot.0.x += margin.left;
			spot.0.y += margin.top;
			let w = ((spot.1.w as isize) - margin.total_on(Axis::Horizontal)).try_into();
			let h = ((spot.1.h as isize) - margin.total_on(Axis::Vertical)).try_into();
			match (w, h) {
				(Ok(w), Ok(h)) => spot.1 = Size::new(w, h),
				_ => None?,
			}
		}
		Some(spot)
	}

	fn get_content_spot(&self) -> Option<Spot> {
		self.get_content_spot_at(self.get_spot())
	}

	#[allow(unused)]
	fn set_spot(&mut self, spot: Spot) -> Void {
		None
	}

	fn validate_spot(&mut self) {
		// do nothing
	}

	#[allow(unused)]
	fn container(&self) -> Option<(Axis, usize)> {
		None
	}

	#[allow(unused)]
	fn event_mask(&self) -> EventMask {
		EventMask::empty()
	}
}

pub type RcNode = Arc<Mutex<dyn Node>>;

/// This utility function wraps a widget in an Arc<Mutex<W>>.
pub fn rc_node<W: Node>(node: W) -> RcNode {
	Arc::new(Mutex::new(node))
}

#[derive(Debug, Copy, Clone)]
pub struct DummyNode;

impl Node for DummyNode {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn describe(&self) -> String {
		String::from("Dummy node")
	}
}

#[cfg(feature = "railway")]
lazy_static! {
	static ref CONTAINER_RWY: StyleRwy = {
		let program = Program::parse(include_bytes!("container.rwy")).unwrap();
		let stack = program.create_stack();
		program.valid().unwrap();
		let (size, margin_radius, parent_rg, parent_ba);
		{
			let arg = |s| program.argument(s).unwrap() as usize;
			size = arg("size");
			margin_radius = arg("margin-radius");
			parent_rg = arg("background-color-red-green");
			parent_ba = arg("background-color-blue-alpha");
		}
		StyleRwy {
			program,
			stack,
			mask: Vec::new(),
			size,
			margin_radius,
			parent_rg,
			parent_ba,
		}
	};
}

#[cfg(feature = "railway")]
#[derive(Debug, Clone)]
pub struct StyleRwy {
	program: Program,
	stack: Vec<Couple>,
	mask: Vec<u8>,
	size: usize,
	margin_radius: usize,
	parent_rg: usize,
	parent_ba: usize,
}

#[derive(Debug, Clone)]
pub struct Container {
	pub children: Vec<RcNode>,
	pub policy: LengthPolicy,
	pub spot: Spot,
	pub axis: Axis,
	pub gap: usize,
	pub margin: Option<usize>,
	pub radius: Option<usize>,
	pub dirty: bool,
	pub style: Option<usize>,
	#[cfg(feature = "railway")]
	pub style_rwy: Option<StyleRwy>,
}

impl Node for Container {
	#[cfg(feature = "railway")]
	fn initialize(&mut self, _: &mut Application, _: &NodePath) -> Result<(), String> {
		if let Some(_) = self.style {
			self.style_rwy = Some(CONTAINER_RWY.clone());
		}
		Ok(())
	}

	fn render(&mut self, app: &mut Application, path: &mut NodePath, style: usize) -> Option<usize> {
		if self.dirty {
			self.dirty = false;
			let (_, size) = self.spot;
			if let Some(i) = self.style {
				#[cfg(feature = "railway")]
				if let Some(rwy) = &mut self.style_rwy {
					if self.margin.is_some() || self.radius.is_some() {
						let parent_bg = app.styles[style].background;
						let c = |i| parent_bg[i] as f32 / 255.0;
						let margin = self.margin.unwrap_or(1);
						let radius = self.radius.unwrap_or(1);
						rwy.stack[rwy.size] = Couple::new(size.w as f32, size.h as f32);
						rwy.stack[rwy.margin_radius] = Couple::new(margin as f32, radius as f32);
						rwy.stack[rwy.parent_rg] = Couple::new(c(0), c(1));
						rwy.stack[rwy.parent_ba] = Couple::new(c(2), c(3));
						rwy.mask.resize(size.w * size.h, 0);
						rwy.program.compute(&mut rwy.stack);
						let (dst, pitch, _) = app.blit(&self.spot, Some(path));
						rwy.program.render::<RWY_PXF_RGBA8888>(&rwy.stack, dst, &mut rwy.mask, size.w, size.h, pitch);
					}
				}
				let this_bg = app.styles[i].background;
				let (mut dst, pitch, _) = app.blit(&self.spot, None);
				let px_width = RGBA * size.w;
				for _ in 0..size.h {
					let (line_dst, dst_next) = dst.split_at_mut(px_width);
					for i in 0..px_width {
						line_dst[i] = this_bg[i % RGBA];
					}
					dst = match dst_next.get_mut(pitch..) {
						Some(dst) => dst,
						None => break,
					};
				}
			}
			if app.debug_containers {
				let (mut dst, pitch, _) = app.blit(&self.spot, Some(path));
				let px_width = RGBA * size.w;
				for i in 0..size.h {
					let (line_dst, dst_next) = dst.split_at_mut(px_width);
					if i == 0 {
						line_dst.fill(255);
					} else {
						line_dst[RGBA..].fill(0);
						line_dst[..RGBA].fill(255);
					}
					dst = dst_next.get_mut(pitch..)?;
				}
			}
		}
		Some(self.style.unwrap_or(style))
	}

	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn margin(&self) -> Option<Margin> {
		self.margin.map(|l| Margin::quad(l as isize))
	}

	fn children(&self) -> &[RcNode] {
		&self.children
	}

	fn policy(&self) -> LengthPolicy {
		self.policy
	}

	fn add_node(&mut self, child: RcNode) -> Result<usize, String> {
		let index = self.children.len();
		self.children.push(child);
		Ok(index)
	}

	fn replace_node(&mut self, index: usize, child: RcNode) -> Result<(), String> {
		match self.children.get_mut(index) {
			Some(addr) => *addr = child,
			None => Err(String::from("No such child :|"))?,
		};
		Ok(())
	}

	fn get_spot(&self) -> Spot {
		self.spot
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.dirty = true;
		self.spot = spot;
		None
	}

	fn set_dirty(&mut self) {
		self.dirty = true;
	}

	fn container(&self) -> Option<(Axis, usize)> {
		Some((self.axis, self.gap))
	}

	fn describe(&self) -> String {
		String::from(match self.axis {
			Axis::Vertical   => "Vertical Container",
			Axis::Horizontal => "Horizontal Container",
		})
	}
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

	pub fn quad(value: isize) -> Self {
		Self {
			top: value,
			bottom: value,
			left: value,
			right: value,
		}
	}

	pub fn total_on(&self, axis: Axis) -> isize {
		match axis {
			Axis::Horizontal => self.left + self.right,
			Axis::Vertical   => self.top + self.bottom,
		}
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

	pub fn complement(self) -> Self {
		match self {
			Axis::Horizontal => Axis::Vertical,
			Axis::Vertical => Axis::Horizontal,
		}
	}
}

pub(crate) trait SameAxisContainerOrNone {
	fn same_axis_or_both_none(self) -> bool;
}

impl SameAxisContainerOrNone for (Option<(Axis, usize)>, Option<(Axis, usize)>) {
	fn same_axis_or_both_none(self) -> bool {
		match self {
			(Some((a, _)), Some((b, _))) => a == b,
			(None, None) => true,
			_ => false,
		}
	}
}
