//! Support for PNG images
//!
//! # List of tags
//!
//! ## `png`
//!
//! A simple node displaying an image decoded from the PNG format.
//!
//! Special Attribute: `file` (name of the asset, no default)

use crate::core::visual::{PixelSource, Ratio, aspect_ratio, LayoutMode, Texture};
use crate::core::visual::{RgbPixelBuffer, RgbaPixelBuffer, PixelBuffer};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::xml::{tag};
use crate::core::event::Event;
use crate::{Vec, Box, HashMap, CheapString, Rc, Error, error};

use png::ColorType;
use png::Decoder;

pub const PNG_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("png")),
    xml_attr_set: Some(&["file"]),
    xml_accepts_children: false,
    handler: png_loader,
};

type PngStorage = HashMap<CheapString, (Ratio, Rc<dyn Texture>)>;

fn png_loader(app: &mut Application, m: MutatorIndex, event: Event) -> Result<(), Error> {
    match event {
        Event::Initialize => {
            let storage = &mut app.storage[usize::from(m)];
            assert!(storage.is_none());

            *storage = Some(Box::new(PngStorage::new()));

            Ok(())
        },
        Event::ParseAsset { asset, bytes, .. } => {
            let parsed = {
                let decoder = Decoder::new(&*bytes);
                let mut reader = decoder.read_info().unwrap();
                let mut buf = Vec::with_capacity(reader.output_buffer_size());
                buf.resize(reader.output_buffer_size(), 0);

                let info = reader.next_frame(&mut buf).unwrap();
                let width = info.width as usize;
                let height = info.height as usize;
                let ratio = aspect_ratio(width, height);

                let texture: Rc<dyn Texture> = match info.color_type {
                    ColorType::Rgb  => Rc::new( RgbPixelBuffer::new(buf.into_boxed_slice(), width, height)),
                    ColorType::Rgba => Rc::new(RgbaPixelBuffer::new(buf.into_boxed_slice(), width, height)),
                    _ => panic!("unsupported PNG color type"),
                };

                (ratio, texture)
            };

            let storage = app.storage[usize::from(m)].as_mut().unwrap();
            let storage: &mut PngStorage = storage.downcast_mut().unwrap();
            storage.insert(asset, parsed);

            Ok(())
        },
        Event::Populate { node_key, .. } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;
            app.request(file, node_key, true)
        },
        Event::AssetLoaded { node_key } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;

            let (ratio, texture) = {
                let storage = app.storage[usize::from(m)].as_ref().unwrap();
                let storage: &PngStorage = storage.downcast_ref().unwrap();
                storage[&file].clone()
            };

            app.view[node_key].foreground = PixelSource::RcTexture(texture);
            app.view[node_key].layout_config.set_dirty(true);

            app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));
            app.invalidate_layout();

            Ok(())
        },
        Event::Resized { .. } => Ok(()),
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}
