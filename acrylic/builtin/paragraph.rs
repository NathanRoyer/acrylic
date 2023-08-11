use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::event::{Handlers, DEFAULT_HANDLERS, UserInputEvent};
use crate::core::visual::{aspect_ratio, LayoutMode, Axis, Pixels};
use crate::core::glyph::{space_width, get_font, load_font_bytes};
use crate::core::app::{Application, UNBREAKABLE_MUTATOR_INDEX};
use crate::core::node::{NodeKey, Mutator, MutatorIndex};
use crate::core::text_edit::{text_edit, break_ws};
use crate::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE,
    Error, error, ArcStr, ro_string, Box,
};

const TEXT: usize = 0;
const FONT: usize = 1;
const SIZE: usize = 2;

pub const PARAGRAPH_MUTATOR: Mutator = Mutator {
    name: ro_string!("ParagraphMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("p"),
        attr_set: &[
            ("text", AttributeValueType::Other, None),
            ("font", AttributeValueType::Other, Some(DEFAULT_FONT_NAME)),
            ("size", AttributeValueType::Pixels, Some(DEFAULT_FONT_SIZE)),
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

pub const UNBREAKABLE_MUTATOR: Mutator = Mutator {
    name: ro_string!("UnbreakableMutator"),
    xml_params: None,
    handlers: Handlers {
        ..DEFAULT_HANDLERS
    },
    storage: None,
};

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;

    let parent = app.view.parent(node_key).ok_or_else(|| error!())?;
    if app.view[parent].layout_config.get_content_axis() != Axis::Vertical {
        let xml_node = &app.xml_tree[xml_node_key];
        let line = xml_node.line.get().unwrap_or(0.into());
        return Err(error!("Paragraph is in an horizontal container; this is invalid! (line {})", line));
    }

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
    let font_size: Pixels = app.attr(node_key, SIZE)?;
    let font_size = font_size.to_num();

    if text.len() > 0 {

        let font = get_font(&mut app.mutators, &font_file).unwrap();

        for unbreakable in break_ws(&text) {
            let new_node = app.view.create();

            let width = font.quick_width(&unbreakable, font_size);
            let ratio = aspect_ratio(width, font_size);
            app.view[new_node].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));

            let factory = Some(UNBREAKABLE_MUTATOR_INDEX.into()).into();
            app.view[new_node].factory = factory;

            app.view.append_children(new_node, node_key);
        }

        let row = Pixels::from_num(font_size);
        let gap = Pixels::from_num(space_width(font_size));
        app.view[node_key].layout_config.set_layout_mode(LayoutMode::Chunks(row));
        app.view[node_key].layout_config.set_content_axis(Axis::Horizontal);
        app.view[node_key].layout_config.set_content_gap(gap);
        app.invalidate_layout();
    }

    Ok(())
}

fn resizer(app: &mut Application, _m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let font_file:       ArcStr = app.attr(node_key, FONT)?;
    let font_size:       Pixels = app.attr(node_key, SIZE)?;
    let text:            ArcStr = app.attr(node_key, TEXT)?;

    let font_size = font_size.to_num();
    let is_focused = Some(node_key) == app.get_focused_node();
    let inherited_style = app.get_inherited_style(node_key)?;

    if text.len() > 0 && !app.debug.skip_glyph_rendering {
        let font = match get_font(&mut app.mutators, &font_file) {
            Some(font) => font,
            None => return Ok(()),
        };

        let mut child = app.view.first_child(node_key).unwrap();
        let mut unbrk_index = 0;
        for unbreakable in break_ws(&text) {
            let cursors = match is_focused {
                true => Some((unbrk_index, app.text_cursors.as_slice())),
                false => None,
            };

            let color = Some(inherited_style.foreground);
            app.view[child].layout_config.set_dirty(true);
            app.view[child].foreground = {
                let mut renderer = font.renderer(color, cursors, font_size);
                renderer.write(&unbreakable);
                renderer.texture()
            };

            child = app.view.next_sibling(child);
            unbrk_index += 1;
        }
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
    let font_size:       Pixels = app.attr(node_key, SIZE)?;
    let text:            ArcStr = app.attr(node_key, TEXT)?;

    let text_path = match app.attr_state_path(node_key, TEXT)? {
        Err(_) => {
            log::error!("Cannot modify state during TextInsert: attribute isn't a state path");
            return Ok(true);
        },
        Ok((attr_path, _)) => attr_path,
    };

    text_edit(true, app, node_key, event, font_file, font_size, text, text_path)
}
