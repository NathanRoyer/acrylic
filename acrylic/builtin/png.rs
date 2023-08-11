use crate::core::visual::{PixelSource, Ratio, aspect_ratio, LayoutMode, Texture};
use crate::core::visual::{RgbPixelArray, RgbaPixelArray};
use crate::core::app::Application;
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::node::{NodeKey, Mutator, MutatorIndex, get_storage};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Vec, Box, HashMap, ArcStr, Rc, Error, ro_string};

use png::ColorType;
use png::Decoder;

const FILE: usize = 0;

pub const PNG_MUTATOR: Mutator = Mutator {
    name: ro_string!("PngMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("png"),
        attr_set: &[ ("file", AttributeValueType::Other, None) ],
        accepts_children: false,
    }),
    handlers: Handlers {
        initializer,
        parser,
        populator,
        finalizer,
        ..DEFAULT_HANDLERS
    },
    storage: None,
};

type PngStorage = HashMap<ArcStr, (Ratio, Rc<dyn Texture>)>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.mutators[usize::from(m)].storage;
    assert!(storage.is_none());

    *storage = Some(Box::new(PngStorage::new()));

    Ok(())
}

fn parser(app: &mut Application, m: MutatorIndex, _node_key: NodeKey, asset: &ArcStr, bytes: Box<[u8]>) -> Result<(), Error> {
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
            ColorType::Rgb  => Rc::new( RgbPixelArray::new(buf.into_boxed_slice(), width, height)),
            ColorType::Rgba => Rc::new(RgbaPixelArray::new(buf.into_boxed_slice(), width, height)),
            _ => panic!("unsupported PNG color type"),
        };

        (ratio, texture)
    };

    let storage: &mut PngStorage = get_storage(&mut app.mutators, m).unwrap();
    storage.insert(asset.clone(), parsed);

    Ok(())
}

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let file: ArcStr = app.attr(node_key, FILE)?;
    app.request(&file, node_key, true)
}

fn finalizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let file: ArcStr = app.attr(node_key, FILE)?;

    let (ratio, texture) = {
        let storage: &mut PngStorage = get_storage(&mut app.mutators, m).unwrap();
        storage.get(&file).unwrap().clone()
    };

    app.view[node_key].foreground = PixelSource::RcTexture(texture);
    app.view[node_key].config.set_dirty(true);

    app.view[node_key].config.set_layout_mode(LayoutMode::AspectRatio(ratio));
    app.invalidate_layout();

    Ok(())
}
