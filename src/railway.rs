use crate::app::Application;
use crate::geometry::aspect_ratio;
use crate::node::rc_node;
use crate::node::NodePath;
use crate::node::Node;
use crate::node::LengthPolicy;
use crate::Spot;
use crate::Point;
use crate::Size;
use crate::Void;
use crate::format;

#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::app::DataRequest;

use railway::Program;
use railway::Address;
use railway::Couple;

use core::any::Any;

use std::string::String;
use std::vec::Vec;
use std::println;
#[cfg(not(feature = "std"))]
use std::print;

/// See [`xml_handler`]
#[derive(Debug, Clone)]
pub struct Railway {
	pub(crate) program: Program,
	pub(crate) stack: Vec<Couple>,
	pub(crate) ratio: f64,
	pub(crate) size_arg: Address,
	pub(crate) time_arg: Option<Address>,
	pub(crate) mask: Vec<u8>,
	pub(crate) node_size: Size,
	pub(crate) position: Point,
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
			.ok_or(String::from("Missing size in railway file"))?;
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
			position: Point::zero(),
			node_size: Size::zero(),
		})
	}
}

impl Node for Railway {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn describe(&self) -> String {
		String::from("Railway file")
	}

	fn policy(&self) -> LengthPolicy {
		LengthPolicy::AspectRatio(self.ratio)
	}

	fn get_spot(&self) -> Spot {
		(self.position, self.node_size)
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.position = spot.0;
		self.node_size = spot.1;
		None
	}

	fn render(&mut self, app: &mut Application, _path: &mut NodePath) -> Void {
		let _ = self.time_arg;
		self.mask.resize(self.node_size.w * self.node_size.h, 0);
		self.stack[self.size_arg as usize] = Couple::new(self.node_size.w as f32, self.node_size.h as f32);
		self.program.compute(&mut self.stack);
		let offset = 4 * ((self.position.y as usize) * app.output.size.w + (self.position.x as usize));
		let pitch = 4 * (app.output.size.w - self.node_size.w);
		let dst = &mut app.output.pixels[offset..];
		self.program.render(&self.stack, dst, &mut self.mask, self.node_size.w, self.node_size.h, pitch);
		None
	}
}

#[derive(Debug, Copy, Clone)]
pub struct RailwayLoader;

impl Node for RailwayLoader {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn describe(&self) -> String {
		String::from("Loading railway file...")
	}

	fn loaded(&mut self, app: &mut Application, path: &NodePath, _: &str, _: usize, data: &[u8]) -> Void {
		let railway = match Railway::new(data) {
			Err(s) => (println!("{}", s), None).1?,
			Ok(r) => r,
		};
		app.replace_node(path, rc_node(railway)).unwrap();
		None
	}
}

/// This function is to be used in [`crate::xml::TreeParser::with`].
/// It parses xml attributes to find an image source, and installs
/// a PngLoader node and a data request for the image. Once the data loads, the [`PngLoader`]
/// instance parses the png image and replaces itself with a [`Bitmap`]
/// containing the decoded image.
#[cfg(feature = "xml")]
pub fn xml_handler(app: &mut Application, path: &mut NodePath, attributes: &[Attribute]) -> Result<(), String> {
	let mut source = Err(String::from("missing src attribute"));

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"src" => source = Ok(value.clone()),
			_ => unexpected_attr(&name)?,
		}
	}

	path.push(app.add_node(path, rc_node(RailwayLoader))?);

	app.data_requests.push(DataRequest {
		node: path.clone(),
		name: source?,
		range: None,
	});

	Ok(())
}
