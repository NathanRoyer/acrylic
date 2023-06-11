use crate::core::visual::{aspect_ratio, LayoutMode, Axis, Pixels};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::glyph::{space_width, get_font, load_font_bytes};
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::node::NodeKey;
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::{Error, error, ArcStr, ro_string, Box, DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE};

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
    let font_size:      Pixels = app.attr(node_key, SIZE)?;
    let font_size = font_size.to_num();

    if text.len() > 0 {

        let font = get_font(&mut app.mutators, &font_file).unwrap();

        for unbreakable in text.split_whitespace() {
            let new_node = app.view.create();

            let width = {
                let mut renderer = font.renderer(None, font_size);
                renderer.write(&unbreakable);
                renderer.width()
            };
            let ratio = aspect_ratio(width, font_size);
            app.view[new_node].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));

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
    let text:      ArcStr = app.attr(node_key, TEXT)?;
    let font_file: ArcStr = app.attr(node_key, FONT)?;
    let font_size:      Pixels = app.attr(node_key, SIZE)?;
    let font_size = font_size.to_num();

    if text.len() > 0 && !app.debug.skip_glyph_rendering {
        let font = match get_font(&mut app.mutators, &font_file) {
            Some(font) => font,
            None => return Ok(()),
        };

        let mut child = app.view.first_child(node_key).unwrap();
        for unbreakable in text.split_whitespace() {
            let color = rgb::RGBA8::new(230, 230, 230, 255);
            app.view[child].layout_config.set_dirty(true);
            app.view[child].foreground = {
                let mut renderer = font.renderer(Some(color), font_size);
                renderer.write(&unbreakable);
                renderer.texture()
            };

            child = app.view.next_sibling(child);
        }
    }

    Ok(())
}
