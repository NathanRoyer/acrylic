use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::event::{Handlers, UserInputEvent, DEFAULT_HANDLERS};
use crate::core::node::{NodeKey, Mutator, MutatorIndex};
use crate::core::visual::{aspect_ratio, LayoutMode};
use crate::core::glyph::{get_font, load_font_bytes};
use crate::core::text_edit::text_edit;
use crate::core::app::Application;
use crate::{DEFAULT_FONT_NAME, Error, ArcStr, ro_string, Box};

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
        user_input_handler,
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
        app.view[node_key].config.set_layout_mode(LayoutMode::AspectRatio(ratio));

        app.invalidate_layout();
    }

    Ok(())
}

fn resizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;

    let inherited_style = app.get_inherited_style(node_key)?;
    let cursors = match Some(node_key) == app.get_focused_node() {
        true => Some((0, app.text_cursors.as_slice())),
        false => None,
    };

    if text.len() > 0 && !app.debug.skip_glyph_rendering {
        let color = Some(inherited_style.foreground);
        let font_size = app.view[node_key].size.h.round().to_num();
        app.view[node_key].config.set_dirty(true);
        app.view[node_key].foreground = {
            let font = get_font(&mut app.mutators, &font_file).unwrap();
            let mut renderer = font.renderer(color, cursors, font_size);
            renderer.write(&text);
            renderer.texture()
        };
    }

    Ok(())
}

fn user_input_handler(
    app: &mut Application,
    _m: MutatorIndex,
    node_key: NodeKey,
    _target: NodeKey,
    event: &UserInputEvent,
) -> Result<bool, Error> {
    let font_file:       ArcStr = app.attr(node_key, FONT)?;
    let text:            ArcStr = app.attr(node_key, TEXT)?;

    let font_size = app.view[node_key].size.h.round().to_num();
    let text_path = match app.attr_state_path(node_key, TEXT)? {
        Err(_) => {
            log::error!("Cannot modify state during TextInsert: attribute isn't a state path");
            return Ok(true);
        },
        Ok((attr_path, _)) => attr_path,
    };

    text_edit(false, app, node_key, event, font_file, font_size, text, text_path)
}
