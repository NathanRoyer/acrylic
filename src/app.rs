use crate::node::Node;
use crate::node::RcNode;
use crate::node::NodePath;
use crate::node::rc_node;
use crate::bitmap::RGBA;
use crate::flexbox::compute_tree;
use crate::PlatformBlit;
use crate::PlatformLog;
use crate::Point;
use crate::Spot;
use crate::Size;
use crate::Void;
use crate::lock;

#[cfg(feature = "text")]
use crate::text::Font;

use core::any::Any;
use core::fmt::Debug;
use core::ops::Range;
use core::ops::Deref;
use core::ops::DerefMut;

use std::string::String;
use std::vec::Vec;
use std::boxed::Box;

#[cfg(feature = "text")]
use std::collections::HashMap;
#[cfg(feature = "text")]
use std::sync::Arc;
#[cfg(feature = "text")]
use std::sync::Mutex;

/// The Application structure represents your application.
/// It has a [`Tree`] containing nodes, a `model` field
/// where you can store your application-specific model,
/// and a vector of [`DataRequest`] where you can add
/// you own data requests (which the platform will handle).
pub struct Application {
	pub view: RcNode,

	pub view_spot: Spot,

	/// Fonts that can be used by widgets to draw glyphs
	#[cfg(feature = "text")]
	pub fonts: HashMap<Option<String>, Arc<Mutex<Font>>>,

	/// Default font size used by textual widgets
	#[cfg(feature = "text")]
	pub default_font_size: usize,

	/// Data requests allow widgets to load external assets,
	/// partially or completely. You can append new ones to
	/// this vector.
	pub data_requests: Vec<DataRequest>,

	/// This field's content is completely up to you. You
	/// should use it to store the global state of your
	/// application.
	pub model: Box<dyn Any>,

	pub platform_log: PlatformLog,

	pub platform_blit: PlatformBlit,

	pub blit_hooks: Vec<(NodePath, Spot)>,

	pub styles: Vec<Style>,

	pub should_recompute: bool,

	pub debug_containers: bool,
}

/// Data requests allow widgets to load external assets,
/// partially or completely.
/// You can append new ones to `app.data_requests`.
#[derive(Debug, Clone, Hash)]
pub struct DataRequest {
	pub node: NodePath,
	pub name: String,
	pub range: Option<Range<usize>>,
}

pub type Color = [u8; RGBA];

#[derive(Debug, Copy, Clone)]
pub struct Style {
	pub background: Color,
	pub foreground: Color,
	pub border: Color,
}

impl Application {
	/// The Application constructor. If you omit the `tree`
	/// argument, it will be initialized to an empty tree.
	pub fn new<M: Any + 'static>(log: PlatformLog, blit: PlatformBlit, model: M, view: impl Node) -> Self {
		#[allow(unused_mut)]
		let mut app = Self {
			view: rc_node(view),
			view_spot: (Point::zero(), Size::zero()),
			#[cfg(feature = "text")]
			fonts: HashMap::new(),
			#[cfg(feature = "text")]
			default_font_size: 30,
			data_requests: Vec::new(),
			model: Box::new(model),
			should_recompute: true,
			debug_containers: false,
			styles: Vec::new(),
			platform_log: log,
			platform_blit: blit,
			blit_hooks: Vec::new(),
		};
		app.initialize_node(app.view.clone(), &mut Vec::new()).unwrap();
		#[cfg(all(feature = "text", feature = "noto-default-font"))]
		{
			let font = Font::from_bytes(include_bytes!("noto-sans-regular.ttf").to_vec());
			app.fonts.insert(None, font);
		}
		app
	}

	/// This getter allows you to get your model as its initial
	/// type. If `M` is the original type of your model, this
	/// will return Some, and None if it is not.
	///
	/// Under the hood, this is a simple downcast.
	pub fn model<M: Any + 'static>(&mut self) -> Option<&mut M> {
		self.model.downcast_mut::<M>()
	}

	/// Adds a font to the font store. If `default` is `true`,
	/// this font will be used by default when textual nodes
	/// are created.
	#[cfg(feature = "text")]
	pub fn add_font(&mut self, name: String, data: Vec<u8>, default: bool) {
		let font = Font::from_bytes(data);
		self.fonts.insert(Some(name), font.clone());
		if default {
			self.fonts.insert(None, font);
		}
	}

	pub fn set_styles(&mut self, styles: Vec<Style>) {
		self.styles = styles;
	}

	pub fn get_node(&self, path: &NodePath) -> Option<RcNode> {
		let mut node = self.view.clone();
		for i in path {
			// todo: get rid of these locks
			let child = {
				let tmp = lock(&node)?;
				tmp.children().get(*i)?.clone()
			};
			node = child;
		}
		Some(node)
	}

	pub fn replace_node(&mut self, path: &NodePath, new_node: RcNode) -> Result<(), String> {
		self.should_recompute = true;
		if let Some(j) = path.last() {
			let mut node = self.view.clone();
			for i in &path[..path.len() - 1] {
				// todo: get rid of these locks
				let child = {
					let tmp = lock(&node).unwrap();
					tmp.children()[*i].clone()
				};
				node = child;
			}
			let mut tmp = lock(&node).unwrap();
			tmp.replace_node(*j, new_node.clone())?;
		} else {
			self.view = new_node.clone();
			let mut view = lock(&self.view).unwrap();
			view.set_spot(self.view_spot);
		}
		let mut path = path.clone();
		self.initialize_node(new_node, &mut path)
	}

	fn set_cont_dirty(node: &mut dyn Node, validate_only: bool) -> Void {
		let children = {
			if validate_only {
				node.validate_spot();
			} else {
				node.set_dirty();
			}
			node.children().to_vec()
		};
		for child in children {
			let mut child = lock(&child)?;
			let child = child.deref_mut();
			Self::set_cont_dirty(child, validate_only);
		}
		None
	}

	pub fn set_spot(&mut self, spot: Spot) {
		if self.view_spot != spot {
			self.view_spot = spot;
			let mut view = lock(&self.view).unwrap();
			let view = view.deref_mut();
			view.set_spot(spot);
			self.should_recompute = true;
			Self::set_cont_dirty(view, false);
		}
	}

	/// This method is called by the platform to request a refresh
	/// of the output. It should be called for every frame.
	pub fn render(&mut self) {
		if self.should_recompute {
			self.log("recomputing layout");
			{
				let mut view = lock(&self.view).unwrap();
				compute_tree(view.deref());
				Self::set_cont_dirty(view.deref_mut(), true);
			}
			for i in 0..self.blit_hooks.len() {
				if let Some(node) = self.get_node(&self.blit_hooks[i].0) {
					let node = lock(&node).unwrap();
					let spot = node.get_content_spot();
					let spot = spot.unwrap_or((Point::zero(), Size::zero()));
					self.blit_hooks[i].1 = spot;
				}
			}
			self.should_recompute = false;
		}
		let mut path = Vec::new();
		self.render_node(self.view.clone(), &mut path, 0);
	}

	fn render_node(&mut self, node: RcNode, path: &mut NodePath, style: usize) {
		let (children, style) = {
			let mut node = lock(&node).unwrap();
			let (_, size) = node.get_spot();
			let mut style = style;
			if size.w > 0 && size.h > 0 {
				style = node.render(self, path, style).unwrap_or(style);
			}
			(node.children().to_vec(), style)
		};
		for i in 0..children.len() {
			path.push(i);
			self.render_node(children[i].clone(), path, style);
			path.pop();
		}
	}

	fn initialize_node(&mut self, node: RcNode, path: &mut NodePath) -> Result<(), String> {
		let children = {
			let mut node = lock(&node).unwrap();
			node.initialize(self, path)?;
			node.children().to_vec()
		};
		for i in 0..children.len() {
			path.push(i);
			self.initialize_node(children[i].clone(), path)?;
			path.pop();
		}
		Ok(())
	}

	pub fn log(&self, message: &str) {
		(self.platform_log)(message)
	}

	pub fn blit<'a>(&'a mut self, node_spot: &'a Spot, path: Option<&'a NodePath>) -> (&'a mut [u8], usize, bool) {
		if let Some(path) = path {
			for (hook_path, hook_spot) in &self.blit_hooks {
				if path.starts_with(hook_path) {
					let (slice, pitch, owned) = (self.platform_blit)(hook_spot, Some(hook_path));
					let (slice, pitch) = sub_spot(slice, pitch, [hook_spot, node_spot]);
					return (slice, pitch, owned);
				}
			}
		}
		(self.platform_blit)(node_spot, path)
	}
}

pub fn sub_spot<'a>(slice: &'a mut [u8], mut pitch: usize, spots: [&Spot; 2]) -> (&'a mut [u8], usize) {
	let [(hp, hs), (np, ns)] = spots;
	let (x, y) = ((np.x - hp.x) as usize, (np.y - hp.y) as usize);
	pitch += RGBA * (hs.w - ns.w);
	let line = pitch + RGBA * ns.w;
	let start = RGBA * x + y * line;
	let stop = start + ns.h * line;
	(&mut slice[start..stop], pitch)
}
