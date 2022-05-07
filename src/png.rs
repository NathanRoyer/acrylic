use crate::application::Application;
use crate::application::DataRequest;
use crate::geometry::aspect_ratio;
use crate::application::rc_widget;
use crate::application::Widget;
use crate::tree::LengthPolicy;
use crate::bitmap::Bitmap;
use crate::xml::Attribute;
use crate::tree::NodeKey;
use crate::bitmap::RGBA;
use crate::Point;
use crate::Size;
use crate::Void;

use png::Decoder;
use png::ColorType;

use core::any::Any;

/// See [`xml_handler`]
#[derive(Debug, Clone)]
pub struct PngLoader;

fn read_png(bytes: &[u8]) -> Bitmap {
	let decoder = Decoder::new(bytes);
	let mut reader = decoder.read_info().unwrap();
	let mut buf = vec![0; reader.output_buffer_size()];
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
	Bitmap {
		size: Size::new(info.width as usize, info.height as usize),
		channels: RGBA,
		pixels,
	}
}

/// This function is to be used in [`crate::xml::TreeParser::with`].
/// It parses xml attributes to find an image source, and installs
/// a PngLoader node and a data request for the image. Once the data loads, the [`PngLoader`]
/// instance parses the png image and replaces itself with a [`Bitmap`]
/// containing the decoded image.
pub fn xml_handler(app: &mut Application, parent: Option<&mut NodeKey>, attributes: &[Attribute]) -> Result<NodeKey, String> {
	let mut source = Err(String::from("missing src attribute"));

	for Attribute { name, value } in attributes {
		match name.as_str() {
			"src" => source = Ok(value.clone()),
			_ => Err(format!("unexpected attribute: {}", name))?,
		}
	}

	let mut node = app.tree.add_node(parent, 3);
	app.tree.set_node_widget(&mut node, Some(rc_widget(PngLoader {})));
	app.tree.set_node_policy(&mut node, Some(LengthPolicy::AspectRatio(1.0)));
	app.tree.set_node_spot(&mut node, Some((Point::zero(), Size::zero())));
	app.data_requests.push(DataRequest {
		node,
		name: source?,
		range: None,
	});
	Ok(node)
}

impl Widget for PngLoader {
	fn as_any(&mut self) -> &mut dyn Any {
		self
	}

	fn loaded(&mut self, app: &mut Application, mut node: NodeKey, _: &str, _: usize, data: &[u8]) -> Void {
		let bitmap = read_png(data);
		let ratio = aspect_ratio(bitmap.size.w, bitmap.size.h);
		app.tree.set_node_widget(&mut node, Some(rc_widget(bitmap)));
		app.tree.set_node_policy(&mut node, Some(LengthPolicy::AspectRatio(ratio)));
		None
	}
}
