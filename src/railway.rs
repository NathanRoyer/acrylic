use crate::app::Application;
use crate::geometry::aspect_ratio;
use crate::node::rc_node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::node::Node;
use crate::node::LengthPolicy;
use crate::Spot;
use crate::Point;
use crate::Size;
use crate::Void;
use crate::format;

#[cfg(feature = "xml")]
use crate::xml::TreeParser;
#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::app::DataRequest;

use railway::Program;
use railway::Address;
use railway::Couple;
use railway::RWY_PXF_RGBA8888;

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
	pub(crate) spot: Spot,
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
			spot: (Point::zero(), Size::zero()),
		})
	}

	pub fn render<const RWY_PXF: u8>(&mut self, app: &mut Application, path: &mut NodePath) -> Void {
		let (dst, pitch, _) = app.blit(&self.spot, Some(path));
		let (_, size) = self.spot;
		let _ = self.time_arg;
		self.mask.resize(size.w * size.h, 0);
		self.stack[self.size_arg as usize] = Couple::new(size.w as f32, size.h as f32);
		self.program.compute(&mut self.stack);
		self.program.render::<RWY_PXF>(&self.stack, dst, &mut self.mask, size.w, size.h, pitch);
		None
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
		self.spot
	}

	fn set_spot(&mut self, spot: Spot) -> Void {
		self.spot = spot;
		None
	}

	fn render(&mut self, app: &mut Application, path: &mut NodePath, _: usize) -> Option<usize> {
		self.render::<RWY_PXF_RGBA8888>(app, path)?;
		Some(0)
	}
}

#[derive(Debug, Clone)]
pub struct RailwayLoader {
	source: String,
}

impl Node for RailwayLoader {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn describe(&self) -> String {
		String::from("Loading railway file...")
	}

	fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
		app.data_requests.push(DataRequest {
			node: path.clone(),
			name: self.source.clone(),
			range: None,
		});
		Ok(())
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
pub fn xml_handler(_: &mut TreeParser, attributes: &[Attribute]) -> Result<Option<RcNode>, String> {
	let mut source = Err(String::from("missing src attribute"));

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"src" => source = Ok(value.clone()),
			_ => unexpected_attr(&name)?,
		}
	}

	Ok(Some(rc_node(RailwayLoader {
		source: source?,
	})))
}
