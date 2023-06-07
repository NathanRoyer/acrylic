use crate::core::visual::{aspect_ratio, LayoutMode};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::glyph::{get_font, load_font_bytes};
use crate::core::xml::XmlNodeKey;
use crate::core::node::NodeKey;
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Error, CheapString, cheap_string, Box};

pub const LABEL_MUTATOR: Mutator = Mutator {
    name: cheap_string("LabelMutator"),
    xml_tag: Some(cheap_string("label")),
    xml_attr_set: Some(&["text", "font"]),
    xml_accepts_children: false,
    handlers: Handlers {
        populator,
        parser,
        finalizer,
        resizer,
        ..DEFAULT_HANDLERS
    },
};

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    if app.attr(node_key, "text", None)?.display_len() > 0 {
        let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
        app.request(font_file.clone(), node_key, true)
    } else {
        Ok(())
    }
}

fn parser(app: &mut Application, _m: MutatorIndex, _node_key: NodeKey, asset: CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
    load_font_bytes(app, asset, bytes)
}

fn finalizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    if app.attr(node_key, "text", None)?.display_len() > 0 {
        let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
        let text = &app.attr(node_key, "text", None)?;

        let font_size = 100;

        let width = {
            let font = get_font(&mut app.storage, &font_file).unwrap();
            let mut renderer = font.renderer(None, font_size);
            renderer.write(text);
            renderer.width()
        };

        let ratio = aspect_ratio(width, font_size);
        app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));

        app.invalidate_layout();
    }

    Ok(())
}

fn resizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    if app.attr(node_key, "text", None)?.display_len() > 0 && !app.debug.skip_glyph_rendering {
        let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
        let text = &app.attr(node_key, "text", None)?;

        let color = rgb::RGBA8::new(255, 255, 255, 255);
        let font_size = app.view[node_key].size.h.round().to_num();
        app.view[node_key].layout_config.set_dirty(true);
        app.view[node_key].foreground = {
            let font = get_font(&mut app.storage, &font_file).unwrap();
            let mut renderer = font.renderer(Some(color), font_size);
            renderer.write(text);
            renderer.texture()
        };
    }

    Ok(())
}
