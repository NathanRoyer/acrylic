use crate::core::visual::{PixelSource, Ratio, aspect_ratio, LayoutMode, Texture};
use crate::core::visual::{RgbPixelArray, RgbaPixelArray};
use crate::core::app::Application;
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::node::{NodeKey, Mutator, MutatorIndex, get_storage};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Box, HashMap, ArcStr, Rc, Error, error, ro_string};

use zune_png::PngDecoder;
use zune_png::zune_core::{result::DecodingResult, colorspace::ColorSpace};

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
    let mut decoder = PngDecoder::new(&*bytes);
    let pixels = match decoder.decode() {
        Err(e) => Err(error!("PNG decoding: {:?}", e)),
        Ok(DecodingResult::U8(vec)) => Ok(vec.into_boxed_slice()),
        Ok(_) => Err(error!("Unsupported PNG format")),
    }?;

    let (w, h) = match decoder.get_dimensions() {
        None => Err(error!("PNG decoding: unknown error")),
        Some(dims) => Ok(dims),
    }?;

    type RCDT = Rc<dyn Texture>;

    let texture = match decoder.get_colorspace() {
        None => Err(error!("PNG decoding: unknown error")),
        Some(ColorSpace::RGB)  => Ok(Rc::new( RgbPixelArray::new(pixels, w, h)) as RCDT),
        Some(ColorSpace::RGBA) => Ok(Rc::new(RgbaPixelArray::new(pixels, w, h)) as RCDT),
        Some(_) => Err(error!("PNG decoding: unknown error")),
    }?;

    let parsed = (aspect_ratio(w, h), texture);

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
