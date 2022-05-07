use crate::tree::NodeKey;
use crate::tree::Tree;
use crate::tree::Event;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::Size;
use crate::Void;

#[cfg(feature = "text")]
use crate::text::Font;

use core::any::Any;
use core::fmt::Debug;
use core::ops::Range;

use std::sync::Arc;
use std::sync::Mutex;

#[cfg(feature = "text")]
use std::collections::HashMap;

/// Once you assign an object implementing Widget
/// to a node, this node can render and react to UI
/// events. Widgets define the behaviour of nodes.
pub trait Widget: Debug + Any + 'static {
	/// `as_any` is required for as long as upcasting coercion is unstable
	fn as_any(&mut self) -> &mut dyn Any;

	/// The `legend` method is called when the platform needs a
	/// textual description of a widget. This helps making
	/// applications accessible to people with disabilities.
	#[allow(unused)]
	fn legend(&mut self, app: &mut Application, node: NodeKey) -> String;

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

	fn legend(&mut self, _: &mut Application, _: NodeKey) -> String {
		String::from("This should not appear on screen. You are probably facing a bug.")
	}
}

/// This utility function wraps a widget in an Arc<Mutex<W>>.
pub fn rc_widget<W: Widget>(widget: W) -> RcWidget {
	Arc::new(Mutex::new(widget))
}

/// The Application structure represents your application.
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
		#[allow(unused_mut)]
		let mut app = Self {
			tree,
			#[cfg(feature = "text")]
			fonts: HashMap::new(),
			#[cfg(feature = "text")]
			default_font_size: 30,
			data_requests: vec![],
			model: Box::new(model),
			output: Bitmap::new(Size::zero(), RGBA),
			view_root,
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

	/// This method is called by the platform to request a refresh
	/// of the output. It should be called for every frame.
	pub fn render(&mut self) -> Void {
		let (position, size) = self.tree.get_node_spot(self.view_root)?;
		if size != self.output.size {
			self.output = Bitmap::new(size, RGBA);
			self.tree.set_node_spot(&mut self.view_root, Some((position, size)));
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
