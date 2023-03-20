use crate::core::visual::{PixelSource, aspect_ratio, LayoutMode};
use crate::core::visual::{RgbPixelBuffer, RgbaPixelBuffer, PixelBuffer};
use crate::core::app::{Application, Mutator};
use crate::core::xml::{tag};
use crate::core::event::Event;
use crate::{Vec, Box, Error, error};

use png::ColorType;
use png::Decoder;

pub const PNG_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("png")),
    xml_attr_set: Some(&["file"]),
    xml_accepts_children: false,
    handler: png_loader,
    storage: None,
};

fn png_loader(app: &mut Application, event: Event) -> Result<(), Error> {
    match event {
        Event::Populate { node_key, .. } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;
            app.request(file, node_key)
        },
        Event::AssetLoaded { node_key } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;
            let bytes = app.get_asset(&file).unwrap();

            let width;
            let height;
            app.view[node_key].foreground = {
                let decoder = Decoder::new(&**bytes);
                let mut reader = decoder.read_info().unwrap();
                let mut buf = Vec::with_capacity(reader.output_buffer_size());
                buf.resize(reader.output_buffer_size(), 0);

                let info = reader.next_frame(&mut buf).unwrap();
                width = info.width as usize;
                height = info.height as usize;

                PixelSource::Texture(match info.color_type {
                    ColorType::Rgb  => Box::new( RgbPixelBuffer::new(buf.into_boxed_slice(), width, height)),
                    ColorType::Rgba => Box::new(RgbaPixelBuffer::new(buf.into_boxed_slice(), width, height)),
                    _ => panic!("unsupported img"),
                })
            };
            app.view[node_key].layout_config.set_dirty(true);

            let ratio = aspect_ratio(width, height);
            app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));
            app.must_check_layout = true;

            Ok(())
        },
        Event::Resized { .. } => Ok(()),
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}
