use crate::core::visual::{aspect_ratio, LayoutMode};
use crate::core::app::Application;
use crate::core::glyph::{get_font, load_font_bytes};
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::node::{NodeKey, Mutator, MutatorIndex};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Error, ArcStr, ro_string, Box, DEFAULT_FONT_NAME};

const TEXT: usize = 0;
const FONT: usize = 1;

pub const LABEL_MUTATOR: Mutator = Mutator {
    name: ro_string!("LabelMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("label"),
        attr_set: &[
            ("text", AttributeValueType::Other, None),
            ("font", AttributeValueType::Other, Some(DEFAULT_FONT_NAME)),
        ],
        accepts_children: false,
    }),
    handlers: Handlers {
        populator,
        parser,
        finalizer,
        resizer,
        ..DEFAULT_HANDLERS
    },
    storage: None,
};

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;

    match text.len() > 0 {
        true => app.request(&font_file, node_key, true),
        false => Ok(()),
    }
}

fn parser(app: &mut Application, _m: MutatorIndex, _node_key: NodeKey, asset: &ArcStr, bytes: Box<[u8]>) -> Result<(), Error> {
    load_font_bytes(app, asset, bytes)
}

fn finalizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;

    if text.len() > 0 {
        let font_size = 100;

        let font = get_font(&mut app.mutators, &font_file).unwrap();
        let width = font.quick_width(&text, font_size);

        let ratio = aspect_ratio(width, font_size);
        app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));

        app.invalidate_layout();
    }

    Ok(())
}

fn resizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;

    if text.len() > 0 && !app.debug.skip_glyph_rendering {

        let color = rgb::RGBA8::new(255, 255, 255, 255);
        let font_size = app.view[node_key].size.h.round().to_num();
        app.view[node_key].layout_config.set_dirty(true);
        app.view[node_key].foreground = {
            let font = get_font(&mut app.mutators, &font_file).unwrap();
            let mut renderer = font.renderer(Some(color), None, font_size);
            renderer.write(&text);
            renderer.texture()
        };
    }

    Ok(())
}
