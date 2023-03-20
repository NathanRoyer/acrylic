use crate::core::visual::{aspect_ratio, LayoutMode, Axis, Pixels};
use crate::core::app::{Application, Mutator};
use crate::core::glyph::{space_width, StrTexture};
use crate::core::xml::{tag};
use crate::core::event::Event;
use crate::{Error, error, CheapString};

pub const LABEL_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("label")),
    xml_attr_set: Some(&["text", "font"]),
    xml_accepts_children: false,
    handler: label,
    storage: None,
};

fn default_font_size() -> CheapString {
    "24.0".into()
}

fn label(app: &mut Application, event: Event) -> Result<(), Error> {
    match event {
        Event::Populate { node_key, .. } => {
            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                app.request(font_file.clone(), node_key)
            } else {
                Ok(())
            }
        },
        Event::AssetLoaded { node_key } => {
            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                let font_bytes = app.get_asset(&font_file).unwrap();

                let font_size = 100;
                let width = {
                    let mut texture = StrTexture::new(&font_bytes, font_size, None)?;
                    texture.write(&app.attr(node_key, "text", None)?);
                    texture.width()
                };

                let ratio = aspect_ratio(width, font_size);
                app.view[node_key].layout_config.set_layout_mode(LayoutMode::AspectRatio(ratio));

                app.must_check_layout = true;
            }

            Ok(())
        },
        Event::Resized { node_key } => {
            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                let font_bytes = app.get_asset(&font_file).unwrap();

                let color = rgb::RGBA8::new(230, 230, 230, 255);
                let font_size = app.view[node_key].size.h.round().to_num();
                app.view[node_key].layout_config.set_dirty(true);
                app.view[node_key].foreground = {
                    let mut texture = StrTexture::new(&font_bytes, font_size, Some(color))?;
                    texture.write(&app.attr(node_key, "text", None)?);
                    texture.texture()
                };
            }

            Ok(())
        },
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}

pub const PARAGRAPH_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("p")),
    xml_attr_set: Some(&["text", "size", "font"]),
    xml_accepts_children: false,
    handler: paragraph,
    storage: None,
};

fn paragraph(app: &mut Application, event: Event) -> Result<(), Error> {
    match event {
        Event::Populate { node_key, xml_node_key } => {
            let xml_node = &app.xml_tree[xml_node_key];

            let parent = app.view.parent(node_key).ok_or_else(|| error!())?;
            if app.view[parent].layout_config.get_content_axis() != Axis::Vertical {
                let line = xml_node.line.get().unwrap_or(0.into());
                return Err(error!("Paragraph is in an horizontal container; this is invalid! (line {})", line));
            }

            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                app.request(font_file.clone(), node_key)
            } else {
                Ok(())
            }
        },
        Event::AssetLoaded { node_key } => {
            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_size = app.attr(node_key, "size", Some(default_font_size()))?.as_usize()?;
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                let font_bytes = app.get_asset(&font_file).unwrap();

                for unbreakable in app.attr(node_key, "text", None)?.split_space() {
                    let new_node = app.view.create();

                    let width = {
                        let mut texture = StrTexture::new(&font_bytes, font_size, None)?;
                        texture.write(&unbreakable);
                        texture.width()
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
                app.must_check_layout = true;
            }

            Ok(())
        },
        Event::Resized { node_key } => {
            if app.attr(node_key, "text", None)?.display_len() > 0 {
                let font_size = app.attr(node_key, "size", Some(default_font_size()))?.as_usize()?;
                let font_file = app.attr(node_key, "font", Some(app.default_font_str.clone()))?.as_str()?;
                let font_bytes = match app.get_asset(&font_file) {
                    Some(bytes) => bytes,
                    None => return Ok(()),
                };

                let mut child = app.view.first_child(node_key).unwrap();
                for unbreakable in app.attr(node_key, "text", None)?.split_space() {
                    let color = rgb::RGBA8::new(230, 230, 230, 255);
                    app.view[child].layout_config.set_dirty(true);
                    app.view[child].foreground = {
                        let mut texture = StrTexture::new(&font_bytes, font_size, Some(color))?;
                        texture.write(&unbreakable);
                        texture.texture()
                    };

                    child = app.view.next_sibling(child);
                }
            }

            Ok(())
        },
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}
