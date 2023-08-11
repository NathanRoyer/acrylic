use crate::core::visual::{aspect_ratio, LayoutMode, Axis, Pixels, SignedPixels};
use crate::core::app::{Application, UNBREAKABLE_MUTATOR_INDEX};
use crate::core::glyph::{space_width, get_font, load_font_bytes};
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::node::{NodeKey, Mutator, MutatorIndex};
use crate::core::event::{Handlers, DEFAULT_HANDLERS, UserInputEvent};
use crate::core::for_each_child;
use crate::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, DEFAULT_CURSOR_NAME,
    Error, error, String, ArcStr, ro_string, Box,
};

use lmfu::json::{JsonFile, Path, Value};

const TEXT: usize = 0;
const FONT: usize = 1;
const SIZE: usize = 2;
const CURSOR: usize = 3;

pub const PARAGRAPH_MUTATOR: Mutator = Mutator {
    name: ro_string!("ParagraphMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("p"),
        attr_set: &[
            ("text", AttributeValueType::Other, None),
            ("font", AttributeValueType::Other, Some(DEFAULT_FONT_NAME)),
            ("size", AttributeValueType::Pixels, Some(DEFAULT_FONT_SIZE)),
            ("cursor", AttributeValueType::Other, Some(DEFAULT_CURSOR_NAME)),
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

fn break_ws(text: &str) -> impl Iterator<Item=&str> {
    text.split(char::is_whitespace)
}

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
    let par_cursor_name: ArcStr = app.attr(node_key, CURSOR)?;
    let font_file:       ArcStr = app.attr(node_key, FONT)?;
    let font_size:       Pixels = app.attr(node_key, SIZE)?;
    let text:            ArcStr = app.attr(node_key, TEXT)?;

    let font_size = font_size.to_num();

    let mut par_cursors = Path::new();
    par_cursors.index_str("_cursors");
    par_cursors.index_str(&par_cursor_name);
    let par_cursors = app.state.iter_array(&par_cursors);
    log::error!("resizer; cursors: {}", par_cursors.clone().count());

    if text.len() > 0 && !app.debug.skip_glyph_rendering {
        let font = match get_font(&mut app.mutators, &font_file) {
            Some(font) => font,
            None => return Ok(()),
        };

        let mut child = app.view.first_child(node_key).unwrap();
        let mut unbrk_index = 0;
        for unbreakable in break_ws(&text) {
            let cursors = Some((unbrk_index, par_cursors.clone()));
            let color = rgb::RGBA8::new(230, 230, 230, 255);
            app.view[child].layout_config.set_dirty(true);
            app.view[child].foreground = {
                let mut renderer = font.renderer(Some(color), cursors, font_size);
                renderer.write(&unbreakable);
                renderer.texture()
            };

            child = app.view.next_sibling(child);
            unbrk_index += 1;
        }
    }

    Ok(())
}

fn get_cursor(state: &JsonFile, cursor_name: &str, index: usize, text: &str) -> (Path, Path, usize, usize, usize) {
    let mut first_cursor = Path::new();
    first_cursor.index_str("_cursors")
                .index_str(&cursor_name)
                .index_num(index);

    let mut unbrk_index_path = first_cursor.clone();
    let mut char_pos_path = first_cursor.clone();
    unbrk_index_path.index_num(0);
    char_pos_path.index_num(1);

    let unbrk_index = match state.get(&unbrk_index_path) {
        Value::Number(unbrk_index) => *unbrk_index as usize,
        _ => 0,
    };

    let char_pos = match state.get(&char_pos_path) {
        Value::Number(char_pos) => *char_pos as usize,
        _ => 0,
    };

    let mut str_index = 0;
    let base = text.as_ptr() as usize;
    if let Some(unbreakable) = break_ws(&text).nth(unbrk_index) {
        let ptr = unbreakable.as_ptr() as usize;
        str_index = ptr - base;

        unbreakable.chars().take(char_pos as _).for_each(|c| str_index += c.len_utf8());
    }

    (unbrk_index_path, char_pos_path, unbrk_index, char_pos, str_index)
}

fn user_input_handler(
    app: &mut Application,
    _m: MutatorIndex,
    node_key: NodeKey,
    _target: NodeKey,
    event: &UserInputEvent,
) -> Result<bool, Error> {
    if let UserInputEvent::QuickAction1 = event {
        // for every unbreakable
        //   if it's vertically contained:
        //     if it's horizontally contained:
        //       find the right char
        //       break
        //     else:
        //       record both sides and their proximity; check and update max
        //
        // if not found:
        //   if max is some:
        //     use max
        //   else:
        //     place cursor at end of text

        let focus = app.get_focus_coords();

        let par_cursor_name: ArcStr = app.attr(node_key, CURSOR)?;
        let font_file:       ArcStr = app.attr(node_key, FONT)?;
        let font_size:       Pixels = app.attr(node_key, SIZE)?;
        let text:            ArcStr = app.attr(node_key, TEXT)?;

        let font = get_font(&mut app.mutators, &font_file).unwrap();
        let font_size = font_size.to_num();

        let mut unbrk_iter = break_ws(&text);
        let mut candidate = None;
        let mut best_distance = SignedPixels::MAX;
        let mut unbrk_index = 0;

        for_each_child!(app.view, node_key, child, {
            let unbreakable = unbrk_iter.next().unwrap();

            let y_min = app.view[child].position.y;
            let y_max = y_min + app.view[child].size.h.to_num::<SignedPixels>();

            if (y_min..y_max).contains(&focus.y) {
                // found the line

                let x_min = app.view[child].position.x;
                let x_max = x_min + app.view[child].size.w.to_num::<SignedPixels>();

                if (x_min..x_max).contains(&focus.x) {
                    // found the unbreakable

                    let x_offset = focus.x - x_min;
                    let char_pos = font.px_to_char_index(x_offset, &unbreakable, font_size);
                    candidate = Some((unbrk_index, char_pos));
                    break;
                } else {
                    let s_distance = (focus.x - x_min).abs();
                    let e_distance = (focus.x - x_max).abs();

                    if s_distance < best_distance || e_distance < best_distance {
                        if s_distance < e_distance {
                            // use start as new candidate
                            best_distance = s_distance;
                            candidate = Some((unbrk_index, 0));
                        } else {
                            // use end as new candidate
                            best_distance = e_distance;
                            candidate = Some((unbrk_index, unbreakable.chars().count()));
                        }
                    }
                }
            }

            unbrk_index += 1;
        });

        let mut par_cursors = Path::new();
        par_cursors.index_str("_cursors");
        par_cursors.index_str(&par_cursor_name);
        app.state.set_array(&par_cursors);

        if let Some((unbrk_index, char_pos)) = candidate {
            let first_cursor = app.state.push(&par_cursors);
            app.state.set_array(&first_cursor);

            let unbrk_index_path = app.state.push(&first_cursor);
            app.state.set_number(&unbrk_index_path, unbrk_index as _);

            let char_pos_path = app.state.push(&first_cursor);
            app.state.set_number(&char_pos_path, char_pos as _);

            app.set_focused_node(node_key)?;
        }

        // trigger buffer refresh
        app.resize(node_key)?;
    }

    else if let UserInputEvent::TextInsert(addition) = event {
        // todo: multi-cursor support

        let par_cursor_name: ArcStr = app.attr(node_key, CURSOR)?;
        let text:            ArcStr = app.attr(node_key, TEXT)?;

        let (
            unbrk_index_path,
            char_pos_path,
            mut unbrk_index,
            mut char_pos,
            insert_pos,
        ) = get_cursor(&app.state, &par_cursor_name, 0, &text);

        let attr_path = match app.attr_state_path(node_key, TEXT)? {
            Err(_) => {
                log::error!("Cannot modify state during TextInsert: attribute isn't a state path");
                return Ok(true);
            },
            Ok((attr_path, _)) => attr_path,
        };

        if let Some(last_new_unb) = break_ws(addition).last() {
            let last_new_unb_len = last_new_unb.len();

            let mut string = String::from(text.as_str());
            string.insert_str(insert_pos, addition);
            app.state.set_string(&attr_path, string.into());

            let num_new_unb = break_ws(addition).count() - 1;
            unbrk_index += num_new_unb;
            char_pos = match num_new_unb > 0 {
                true => last_new_unb_len,
                false => char_pos + last_new_unb_len,
            };

            app.state.set_number(&unbrk_index_path, unbrk_index as _);
            app.state.set_number(&char_pos_path, char_pos as _);

            app.reload_view();
        }
    }

    else if let UserInputEvent::TextDelete(deletion) = event {
        // todo: multi-cursor support

        let par_cursor_name: ArcStr = app.attr(node_key, CURSOR)?;
        let text:            ArcStr = app.attr(node_key, TEXT)?;

        #[allow(unused_assignments)]
        let (
            unbrk_index_path,
            char_pos_path,
            mut unbrk_index,
            mut char_pos,
            del_pos,
        ) = get_cursor(&app.state, &par_cursor_name, 0, &text);

        let attr_path = match app.attr_state_path(node_key, TEXT)? {
            Err(_) => {
                log::error!("Cannot modify state during TextInsert: attribute isn't a state path");
                return Ok(true);
            },
            Ok((attr_path, _)) => attr_path,
        };

        let del_range;

        if *deletion < 0 {
            let new_cursor = del_pos.checked_sub(deletion.abs() as _).unwrap_or(0);
            del_range = new_cursor..del_pos;

            if let Some(substring) = text.get(..new_cursor) {
                if let Some(last_new_unb) = break_ws(substring).last() {
                    char_pos = last_new_unb.len();
                    unbrk_index = break_ws(substring).count() - 1;
                } else {
                    unbrk_index = 0;
                    char_pos = 0;
                }

                app.state.set_number(&unbrk_index_path, unbrk_index as _);
                app.state.set_number(&char_pos_path, char_pos as _);
            }
        } else {
            del_range = del_pos..(del_pos + (*deletion as usize));
        }

        if text.get(del_range.clone()).is_none() {
            log::error!("Invalid deletion offset");
            return Ok(true);
        }

        let mut string = String::from(text.as_str());
        string.replace_range(del_range, "");
        app.state.set_string(&attr_path, string.into());

        app.reload_view();
    }

    else if let UserInputEvent::FocusLoss = event {
        let par_cursor_name: ArcStr = app.attr(node_key, CURSOR)?;

        let mut par_cursors = Path::new();
        par_cursors.index_str("_cursors");
        par_cursors.index_str(&par_cursor_name);

        app.state.remove(&par_cursors);

        // trigger buffer refresh
        app.resize(node_key)?;
    }

    Ok(false)
}
