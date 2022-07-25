use crate::app::Application;
use crate::app::DataRequest;
use crate::bitmap::Bitmap;
use crate::bitmap::RGBA;
use crate::geometry::aspect_ratio;
use crate::node::node_box;
use crate::node::Node;
use crate::node::NodePathSlice;
use crate::node::NodeBox;
use crate::node::RenderReason;
use crate::Size;
use crate::Status;

#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::TreeParser;

use png::ColorType;
use png::Decoder;

use core::any::Any;

use alloc::string::String;
use alloc::vec::Vec;

/// [`Node`] implementor which makes a request to
/// the contained source then replaces itself with
/// a [`Bitmap`] once data has been loaded and parsed.
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
        }
        ColorType::Rgba => buf,
        _ => panic!("unsupported img"),
    };
    let size = Size::new(info.width as usize, info.height as usize);
    Bitmap {
        size,
        channels: RGBA,
        spot_size: Size::zero(),
        pixels,
        cache: Vec::new(),
        margin: None,
        ratio: aspect_ratio(size.w, size.h),
        render_cache: [None, None],
        render_reason: RenderReason::Resized,
    }
}

impl Node for PngLoader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn describe(&self) -> String {
        String::from("Loading PNG image...")
    }

    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), String> {
        app.data_requests.push(DataRequest {
            node: path.to_vec(),
            name: self.source.clone(),
            range: None,
        });
        Ok(())
    }

    fn loaded(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        _: &str,
        _: usize,
        data: &[u8],
    ) -> Status {
        app.replace_kidnapped(path, node_box(read_png(data)));
        Ok(())
    }
}

/// XML tag for PNG Images.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`PngLoader`] node.
///
/// ```xml
/// <png src="img/image0.png" />
/// ```
///
/// The `src` attribute is mandatory and must point to a PNG image asset.
#[cfg(feature = "xml")]
pub fn xml_load_png(
    _: &mut TreeParser,
    attributes: &[Attribute],
) -> Result<Option<NodeBox>, String> {
    let mut source = Err(String::from("missing src attribute"));

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "src" => source = Ok(value.clone()),
            _ => unexpected_attr(&name)?,
        }
    }

    Ok(Some(node_box(PngLoader { source: source? })))
}
