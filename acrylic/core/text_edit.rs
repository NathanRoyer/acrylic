use crate::core::visual::{Pixels, SignedPixels};
use crate::core::event::UserInputEvent;
use crate::{Error, error, String, ArcStr};
use crate::core::app::Application;
use crate::core::glyph::get_font;
use crate::core::for_each_child;
use crate::core::node::NodeKey;

use lmfu::json::Path;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub unbreakable: usize,
    pub char_pos: usize,
}

pub fn break_ws(text: &str) -> impl Iterator<Item=&str> {
    text.split(char::is_whitespace)
}

fn get_cursor(
    text_cursors: &[Cursor],
    paragraph: bool,
    text: &str,
) -> Result<(Cursor, usize), Error> {
    let cursor = match text_cursors.get(0) {
        Some(cursor) => Ok(*cursor),
        None => Err(error!("TextInsert but no cursor?")),
    }?;

    let maybe_unb = match paragraph {
        true => break_ws(text).nth(cursor.unbreakable),
        false => Some(text),
    };

    let base = text.as_ptr() as usize;
    let mut str_index = 0;
    if let Some(unbreakable) = maybe_unb {
        let ptr = unbreakable.as_ptr() as usize;
        str_index = ptr - base;

        unbreakable.chars().take(cursor.char_pos as _).for_each(|c| str_index += c.len_utf8());
    }

    Ok((cursor, str_index))
}

pub fn text_edit(
    paragraph: bool,
    app: &mut Application,
    node_key: NodeKey,
    event: &UserInputEvent,
    font_file: ArcStr,
    font_size: Pixels,
    text: ArcStr,
    text_path: Path,
) -> Result<bool, Error> {
    let mut handled = false;

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

        let font = get_font(&mut app.mutators, &font_file).unwrap();
        let font_size = font_size.to_num();

        let mut candidate = None;
        let mut best_distance = SignedPixels::MAX;

        let mut check = |unbreakable: &str, unbrk_index, node_key: NodeKey| {
            let y_min = app.view[node_key].position.y;
            let y_max = y_min + app.view[node_key].size.h.to_num::<SignedPixels>();

            if (y_min..y_max).contains(&focus.y) {
                // found the line

                let x_min = app.view[node_key].position.x;
                let x_max = x_min + app.view[node_key].size.w.to_num::<SignedPixels>();

                if (x_min..x_max).contains(&focus.x) {
                    // found the unbreakable

                    let x_offset = focus.x - x_min;
                    let char_pos = font.px_to_char_index(x_offset, unbreakable, font_size);
                    candidate = Some((unbrk_index, char_pos));
                    return true;
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

            return false;
        };

        if paragraph {
            let mut unbrk_iter = break_ws(&text);
            let mut unbrk_index = 0;

            for_each_child!(app.view, node_key, child, {
                let unbreakable = unbrk_iter.next().unwrap();

                if check(unbreakable, unbrk_index, child) {
                    break;
                }

                unbrk_index += 1;
            });
        } else {
            check(&text, 0, node_key);
        }

        if let Some((unbrk_index, char_pos)) = candidate {
            app.text_cursors.clear();
            app.text_cursors.push(Cursor {
                unbreakable: unbrk_index,
                char_pos,
            });

            app.set_focused_node(node_key)?;
        }

        // trigger buffer refresh
        app.resize(node_key)?;

        handled = true;
    }

    else if let UserInputEvent::TextInsert(addition) = event {
        // todo: multi-cursor support

        let (
            mut cursor,
            insert_pos,
        ) = get_cursor(&app.text_cursors, paragraph, &text)?;

        let maybe_unb = match paragraph {
            true => break_ws(addition).last(),
            false => addition.len().checked_sub(1).map(|_| *addition),
        };

        if let Some(last_new_unb) = maybe_unb {
            let last_new_unb_len = last_new_unb.len();

            let mut string = String::from(text.as_str());
            string.insert_str(insert_pos, addition);
            app.state.set_string(&text_path, string.into());

            let num_new_unb = match paragraph {
                true => break_ws(addition).count() - 1,
                false => 0,
            };

            cursor.unbreakable += num_new_unb;
            cursor.char_pos = match num_new_unb > 0 {
                true => last_new_unb_len,
                false => cursor.char_pos + last_new_unb_len,
            };

            app.text_cursors[0] = cursor;

            app.reload_view();
        }

        handled = true;
    }

    else if let UserInputEvent::TextDelete(deletion) = event {
        // todo: multi-cursor support

        // #[allow(unused_assignments)]
        let (
            mut cursor,
            del_pos,
        ) = get_cursor(&app.text_cursors, paragraph, &text)?;

        let del_range;

        if *deletion < 0 {
            let new_cursor = del_pos.checked_sub(deletion.abs() as _).unwrap_or(0);
            del_range = new_cursor..del_pos;

            if let Some(substring) = text.get(..new_cursor) {
                let maybe_unb = match paragraph {
                    true => break_ws(substring).last(),
                    false => substring.len().checked_sub(1).map(|_| substring),
                };

                if let Some(last_new_unb) = maybe_unb {
                    cursor.char_pos = last_new_unb.len();
                    cursor.unbreakable = match paragraph {
                        true => break_ws(substring).count() - 1,
                        false => 0,
                    };
                } else {
                    cursor.unbreakable = 0;
                    cursor.char_pos = 0;
                }

                app.text_cursors[0] = cursor;
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
        app.state.set_string(&text_path, string.into());

        app.reload_view();

        handled = true;
    }

    else if let UserInputEvent::FocusLoss = event {
        app.text_cursors.clear();

        // trigger buffer refresh
        app.resize(node_key)?;

        handled = true;
    }

    Ok(handled)
}
