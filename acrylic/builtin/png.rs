use crate::core::visual::{PixelSource, Ratio, aspect_ratio, LayoutMode, Texture};
use crate::core::visual::{RgbPixelBuffer, RgbaPixelBuffer, PixelBuffer};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::xml::XmlNodeKey;
use crate::core::node::NodeKey;
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Vec, Box, HashMap, CheapString, Rc, Error, cheap_string};

use png::ColorType;
use png::Decoder;

pub const PNG_MUTATOR: Mutator = Mutator {
    name: cheap_string("PngMutator"),
    xml_tag: Some(cheap_string("png")),
    xml_attr_set: Some(&["file"]),
    xml_accepts_children: false,
    handlers: Handlers {
        initializer,
        parser,
        populator,
        finalizer,
        ..DEFAULT_HANDLERS
    },
};

type PngStorage = HashMap<CheapString, (Ratio, Rc<dyn Texture>)>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.storage[usize::from(m)];
    assert!(storage.is_none());

    *storage = Some(Box::new(PngStorage::new()));

    Ok(())
}

fn parser(app: &mut Application, m: MutatorIndex, _node_key: NodeKey, asset: &CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
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
    storage.insert(asset.clone(), parsed);

    Ok(())
}

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;
    app.request(&file, node_key, true)
}

fn finalizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;

    let (ratio, texture) = {
        let storage = app.storage[usize::from(m)].as_ref().unwrap();
        let storage: &PngStorage = storage.downcast_ref().unwrap();
        storage.get(&file).unwrap().clone()
    };

    app.view[node_key].foreground = PixelSource::RcTexture(texture);
    app.view[node_key].layout_config.set_dirty(true);

    app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));
    app.invalidate_layout();

    Ok(())
}
