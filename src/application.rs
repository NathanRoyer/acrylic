use crate::tree::NodeKey;
use crate::tree::Tree;
use crate::tree::Event;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::Size;
use crate::Void;
// use crate::loader::Loader;

use std::any::Any;
use std::sync::Arc;
use std::sync::Mutex;
use std::fmt::Debug;
use std::ops::Range;

pub trait Widget: Debug + Any + 'static {
	/// `as_any` is required for as long as upcasting coercion is unstable
	fn as_any(&mut self) -> &mut dyn Any;

	#[allow(unused)]
	fn render(&mut self, app: &mut Application, node: NodeKey) -> Void {
		None
	}

	#[allow(unused)]
	fn handle(&mut self, app: &mut Application, node: NodeKey, event: Event) -> Void {
		None
	}
}

pub type RcWidget = Arc<Mutex<dyn Widget>>;

#[derive(Debug, Copy, Clone, Default)]
pub struct DummyWidget;

impl Widget for DummyWidget {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}
}

pub fn rc_widget<W: Widget>(widget: W) -> RcWidget {
	Arc::new(Mutex::new(widget))
}

#[derive(Debug)]
pub struct Application {
	pub tree: Tree,
	pub data_requests: Vec<DataRequest>,
	pub model: Box<dyn Any>,
	pub output: Bitmap,
}

#[derive(Debug, Clone, Hash)]
pub struct DataRequest {
	pub node: NodeKey,
	pub name: String,
	pub range: Option<Range<usize>>,
}

impl Application {
	pub fn new<M: Any + 'static>(tree: Option<Tree>, model: M) -> Self {
		let tree = match tree {
			Some(tree) => tree,
			None => Tree::new(),
		};
		Self {
			tree,
			data_requests: vec![],
			model: Box::new(model),
			output: Bitmap::new(Size::zero(), RGBA),
		}
	}

	pub fn model<M: Any + 'static>(&mut self) -> Option<&mut M> {
		self.model.downcast_mut::<M>()
	}

	pub fn render(&mut self, root: NodeKey) -> Void {
		let size = self.tree.get_node_spot(root)?.1;
		if size != self.output.size {
			self.output = Bitmap::new(size, RGBA);
		} else {
			self.output.pixels.fill(0);
		}
		self.render_cont(root)
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
