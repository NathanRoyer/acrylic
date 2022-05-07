use crate::tree::NodeKey;
use crate::tree::Tree;
use crate::tree::Event;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::Size;
use crate::Void;

use core::any::Any;
use core::fmt::Debug;
use core::ops::Range;

use std::sync::Arc;
use std::sync::Mutex;

/// Once you assign an object implementing Widget
/// to a node, this node can render and react to UI
/// events. Widgets define the behaviour of nodes.
pub trait Widget: Debug + Any + 'static {
	/// `as_any` is required for as long as upcasting coercion is unstable
	fn as_any(&mut self) -> &mut dyn Any;

	/// The `render` method is called when the platform
	/// needs to refresh the screen. Using `app.tree`, one
	/// can manipulate the node identified by the `node` argument.
	#[allow(unused)]
	fn render(&mut self, app: &mut Application, node: NodeKey) -> Void {
		None
	}

	/// The `handle` method is called when the platform forwards an event
	/// to the application. Using `app.tree`, one can manipulate the node
	/// identified by the `node` argument in reaction.
	///
	/// To receive events via this interface, you must first initialize
	/// the node using [`Tree::set_node_handler`].
	#[allow(unused)]
	fn handle(&mut self, app: &mut Application, node: NodeKey, event: Event) -> Void {
		None
	}

	/// Once you add [`DataRequest`]s to `app.data_requests`, the platform
	/// should fetch the data you requested. Once it has fetched the data,
	/// It will call the `loaded` method.
	#[allow(unused)]
	fn loaded(&mut self, app: &mut Application, node: NodeKey, name: &str, offset: usize, data: &[u8]) -> Void {
		None
	}
}

pub type RcWidget = Arc<Mutex<dyn Widget>>;

/// This type implementing [`Widget`] will not
/// do anything in reaction to rendering calls
/// or event appearance. It is used for text
/// rendering, where glyphs which have not yet
/// been loaded have this instead of a real widget.
/// This prevents re-allocating memory space
/// once the real widget is to be added.
#[derive(Debug, Copy, Clone, Default)]
pub struct DummyWidget;

impl Widget for DummyWidget {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

/// This utility function wraps a widget in an Arc<Mutex<W>>.
pub fn rc_widget<W: Widget>(widget: W) -> RcWidget {
	Arc::new(Mutex::new(widget))
}

/// The Application structure represent your application.
/// It has a [`Tree`] containing nodes, a `model` field
/// where you can store your application-specific model,
/// and a vector of [`DataRequest`] where you can add
/// you own data requests (which the platform will handle).
///
/// The tree can actually contain multiple independent views,
/// so the `view_root` field helps setting one as the
/// current one that the platform should select.
#[derive(Debug)]
pub struct Application {
	/// The tree containing all nodes in a tree structure.
	/// It can contain multiple independent views/trees,
	/// so this could actually be called a forest.
	pub tree: Tree,

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

	/// This is used by the platform to locate the root
	/// node of the view in the tree. Setting an invalid
	/// value here may cause undefined behaviour, but this
	/// should change in the future.
	pub view_root: NodeKey,
}

/// Data requests allow widgets to load external assets,
/// partially or completely.
/// You can append new ones to `app.data_requests`.
#[derive(Debug, Clone, Hash)]
pub struct DataRequest {
	pub node: NodeKey,
	pub name: String,
	pub range: Option<Range<usize>>,
}

impl Application {
	/// The Application constructor. If you omit the `tree`
	/// argument, it will be initialized to an empty tree.
	pub fn new<M: Any + 'static>(tree: Option<Tree>, model: M, view_root: NodeKey) -> Self {
		let tree = match tree {
			Some(tree) => tree,
			None => Tree::new(),
		};
		Self {
			tree,
			data_requests: vec![],
			model: Box::new(model),
			output: Bitmap::new(Size::zero(), RGBA),
			view_root,
		}
	}

	/// This getter allows you to get your model as its initial
	/// type. If `M` is the original type of your model, this
	/// will return Some, and None if it is not.
	///
	/// Under the hood, this is a simple downcast.
	pub fn model<M: Any + 'static>(&mut self) -> Option<&mut M> {
		self.model.downcast_mut::<M>()
	}

	/// This method is called by the platform to request a refresh
	/// of the output. It should be called for every frame.
	pub fn render(&mut self) -> Void {
		let size = self.tree.get_node_spot(self.view_root)?.1;
		if size != self.output.size {
			self.output = Bitmap::new(size, RGBA);
		} else {
			self.output.pixels.fill(0);
		}
		self.render_cont(self.view_root)
	}

	fn render_cont(&mut self, node: NodeKey) -> Void {
		for i in self.tree.children(node) {
			self.render_cont(i);
		}
		self.render_node(node)
	}

	fn render_node(&mut self, node: NodeKey) -> Void {
		let widget = self.tree.get_node_widget(node)?;
		let mut widget = widget.lock().ok()?;
		widget.render(self, node);
		Some(())
	}
}
