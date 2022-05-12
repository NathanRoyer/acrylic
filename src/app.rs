use crate::node::Axis;
use crate::node::Container;
use crate::node::LengthPolicy;
use crate::node::RcNode;
use crate::node::NodePath;
use crate::node::rc_node;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::flexbox::compute_tree;
use crate::Point;
use crate::Size;
use crate::Void;
use crate::lock;

#[cfg(feature = "text")]
use crate::text::Font;

use core::any::Any;
use core::fmt::Debug;
use core::ops::Range;
use core::ops::Deref;

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
#[derive(Debug)]
pub struct Application {
	pub view: RcNode,

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

	/// This [`Bitmap`] is used to store the final frame of
	/// the application, to be rendered by the platform.
	pub output: Bitmap,

	pub should_recompute: bool,
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

impl Application {
	/// The Application constructor. If you omit the `tree`
	/// argument, it will be initialized to an empty tree.
	pub fn new<M: Any + 'static>(model: M) -> Self {
		#[allow(unused_mut)]
		let mut app = Self {
			view: rc_node(Container {
				children: Vec::new(),
				policy: LengthPolicy::Available(1.0),
				spot: (Point::zero(), Size::zero()),
				axis: Axis::Horizontal,
				gap: 0,
			}),
			#[cfg(feature = "text")]
			fonts: HashMap::new(),
			#[cfg(feature = "text")]
			default_font_size: 30,
			data_requests: Vec::new(),
			model: Box::new(model),
			output: Bitmap::new(Size::zero(), RGBA, None),
			should_recompute: true,
		};
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

	pub fn get_node(&mut self, path: &NodePath) -> Option<RcNode> {
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

	pub fn add_node(&mut self, path: &NodePath, child: RcNode) -> Result<usize, String> {
		self.should_recompute = true;
		let node = self.get_node(path).ok_or(String::from("No child at that path"))?;
		let i = {
			let mut node = lock(&node).unwrap();
			node.add_node(self, child.clone())?
		};
		let mut child = lock(&child).unwrap();
		child.attach(self, path);
		Ok(i)
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
			tmp.replace_node(self, *j, new_node.clone())?;
		} else {
			self.view = new_node.clone();
		}
		let mut new_node = lock(&new_node).unwrap();
		new_node.attach(self, path);
		Ok(())
	}

	/// This method is called by the platform to request a refresh
	/// of the output. It should be called for every frame.
	pub fn render(&mut self) -> Void {
		{
			let mut view = lock(&self.view)?;
			let (position, size) = view.get_spot();
			if size != self.output.size {
				self.output = Bitmap::new(size, RGBA, None);
				view.set_spot((position, size));
			} else {
				self.output.pixels.fill(0);
			}
			if self.should_recompute {
				compute_tree(view.deref());
				self.should_recompute = false;
			}
		}
		let mut path = Vec::new();
		self.render_node(self.view.clone(), &mut path)
	}

	fn render_node(&mut self, node: RcNode, path: &mut NodePath) -> Void {
		let children = {
			let mut node = lock(&node)?;
			node.render(self, path);
			node.children().to_vec()
		};
		for i in 0..children.len() {
			path.push(i);
			self.render_node(children[i].clone(), path);
			path.pop();
		}
		None
	}
}
