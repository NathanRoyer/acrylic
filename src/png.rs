use crate::app::Application;
use crate::node::rc_node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::node::Node;
use crate::geometry::aspect_ratio;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::Point;
use crate::Size;
use crate::Void;

#[cfg(feature = "xml")]
use crate::xml::TreeParser;
#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::app::DataRequest;

use png::Decoder;
use png::ColorType;

use core::any::Any;

use std::string::String;
use std::vec::Vec;

/// See [`xml_handler`]
#[derive(Debug, Clone)]
pub struct PngLoader {
	source: String,
}

fn read_png(bytes: &[u8]) -> Bitmap {
	let decoder = Decoder::new(bytes);
	let mut reader = decoder.read_info().unwrap();
	let mut buf = Vec::with_capacity(reader.output_buffer_size());
	buf.resize(reader.output_buffer_size(), 0);
	let info = reader.next_frame(&mut buf).unwrap();
	let len = (info.width * info.height) as usize;
	let pixels = match info.color_type {
		ColorType::Rgb => {
			let mut pixels = Vec::with_capacity(len * 4);
			for i in 0..len {
				let j = i * 3;
				pixels.push(buf[j + 0]);
				pixels.push(buf[j + 1]);
				pixels.push(buf[j + 2]);
				pixels.push(u8::MAX);
			}
			pixels
		},
		ColorType::Rgba => buf,
		_ => panic!("unsupported img"),
	};
	let size = Size::new(info.width as usize, info.height as usize);
	Bitmap {
		size,
		channels: RGBA,
		spot: (Point::zero(), Size::zero()),
		pixels,
		dirty: true,
		cache: Vec::new(),
		margin: None,
		ratio: aspect_ratio(size.w, size.h),
	}
}

impl Node for PngLoader {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn describe(&self) -> String {
		String::from("Loading PNG image...")
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
		app.replace_node(path, rc_node(read_png(data))).unwrap();
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

	Ok(Some(rc_node(PngLoader {
		source: source?,
	})))
}
