use crate::app::Application;
use crate::app::DataRequest;
use crate::geometry::aspect_ratio;
use crate::app::rc_widget;
use crate::app::Widget;
use crate::tree::LengthPolicy;
use crate::tree::NodeKey;
use crate::Point;
use crate::Size;
use crate::Void;

#[cfg(feature = "xml")]
use crate::xml::Attribute;

use railway::Program;
use railway::Address;
use railway::Couple;

use core::any::Any;

/// See [`xml_handler`]
#[derive(Debug, Clone)]
pub struct Railway {
	pub(crate) program: Program,
	pub(crate) stack: Vec<Couple>,
	pub(crate) ratio: f64,
	pub(crate) size_arg: Address,
	pub(crate) time_arg: Option<Address>,
	pub(crate) mask: Vec<u8>,
	// TODO later: theming
}

impl Railway {
	pub fn new(bytes: &[u8]) -> Result<Self, String> {
		let program = match Program::parse(bytes) {
			Ok(p) => p,
			Err(e) => Err(format!("{:?}", e))?,
		};
		let stack = program.create_stack();
		program.valid().ok_or(format!("Invalid railway file"))?;
		let size_arg = program
			.argument("size")
			.ok_or(format!("Missing size in railway file"))?;
		let size = stack[size_arg as usize];
		let ratio = aspect_ratio(size.x as usize, size.y as usize);
		let time_arg = program.argument("time");
		Ok(Self {
			program,
			stack,
			ratio,
			size_arg,
			time_arg,
			mask: Vec::new(),
		})
	}
}

impl Widget for Railway {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn legend(&mut self, _: &mut Application, _: NodeKey) -> String {
		String::from("Railway file")
	}

	fn render(&mut self, app: &mut Application, node: NodeKey) -> Void {
		let (position, size) = app.tree.get_node_spot(node)?;
		let _ = self.time_arg;
		self.mask.resize(size.w * size.h, 0);
		self.stack[self.size_arg as usize] = Couple::new(size.w as f32, size.h as f32);
		self.program.compute(&mut self.stack);
		let offset = (position.y as usize) * app.output.size.w + (position.x as usize);
		let pitch = app.output.size.w - size.w;
		let dst = &mut app.output.pixels[offset..];
		self.program.render(&self.stack, dst, &mut self.mask, size.w, pitch);
		None
	}
}

#[derive(Debug, Copy, Clone)]
pub struct RailwayLoader;

impl Widget for RailwayLoader {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn legend(&mut self, _: &mut Application, _: NodeKey) -> String {
		String::from("Loading railway file...")
	}

	fn loaded(&mut self, app: &mut Application, mut node: NodeKey, _: &str, _: usize, data: &[u8]) -> Void {
		let railway = match Railway::new(data) {
			Err(s) => (println!("{}", s), None).1?,
			Ok(r) => r,
		};
		app.tree.set_node_policy(&mut node, Some(LengthPolicy::AspectRatio(railway.ratio)));
		app.tree.set_node_widget(&mut node, Some(rc_widget(railway)));
		app.tree.compute_flexbox(app.tree.get_node_root(node));
		None
	}
}

/// This function is to be used in [`crate::xml::TreeParser::with`].
/// It parses xml attributes to find an image source, and installs
/// a PngLoader node and a data request for the image. Once the data loads, the [`PngLoader`]
/// instance parses the png image and replaces itself with a [`Bitmap`]
/// containing the decoded image.
#[cfg(feature = "xml")]
pub fn xml_handler(app: &mut Application, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	let mut source = Err(String::from("missing src attribute"));

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"src" => source = Ok(value.clone()),
			_ => Err(format!("unexpected attribute: {}", name))?,
		}
	}

	let mut node = app.tree.add_node(parent, 3);
	app.tree.set_node_widget(&mut node, Some(rc_widget(RailwayLoader {})));
	app.tree.set_node_policy(&mut node, Some(LengthPolicy::AspectRatio(1.0)));
	app.tree.set_node_spot(&mut node, Some((Point::zero(), Size::zero())));
	app.data_requests.push(DataRequest {
		node,
		name: source?,
		range: None,
	});
	Ok(node)
}
