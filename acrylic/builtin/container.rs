use crate::core::app::{Application, Mutator, MutatorIndex, get_storage};
use crate::core::event::{Handlers, DEFAULT_HANDLERS, UserInputEvent};
use crate::core::node::NodeKey;
use crate::core::xml::XmlNodeKey;
use crate::core::visual::{SignedPixels, Margin, Axis, LayoutMode, PixelSource, RgbaPixelBuffer, PixelBuffer};
use crate::core::state::{StateValue, StatePathStep, path_steps};
use crate::core::style::DEFAULT_STYLE;
use crate::core::{for_each_child, rgb::FromSlice};
use crate::core::layout::{get_scroll, scroll};
use crate::{Error, error, Hasher, Box, Vec, cheap_string};
use core::{ops::Deref, hash::Hasher as _};
use oakwood::NodeKey as _;

use railway::{NaiveRenderer, computing::{Couple, C_ZERO}};

type R = NaiveRenderer<&'static [u8]>;

fn parse_tag(app: &mut Application, node: NodeKey, tag: &str) -> Result<(Axis, LayoutMode), Error> {
    let axis = match &tag[..2] {
        "h-" => Axis::Horizontal,
        "v-" => Axis::Vertical,
        _ => unreachable!(),
    };

    let mode = match &tag[2..] {
        "fixed" => LayoutMode::Fixed(app.attr(node, "length", None)?.as_pixels()?),
        "chunks" => LayoutMode::Chunks(app.attr(node, "row", None)?.as_pixels()?),
        "ratio" => LayoutMode::AspectRatio(app.attr(node, "ratio", None)?.as_ratio()?),
        "rem" => LayoutMode::Remaining(app.attr(node, "weight", Some("1".into()))?.as_ratio()?),
        "wrap" => LayoutMode::WrapContent,
        _ => unreachable!(),
    };

    Ok((axis, mode))
}

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.storage[usize::from(m)];
    assert!(storage.is_none());

    let railway = R::parse(include_bytes!(concat!(env!("OUT_DIR"), "/container.rwy"))).unwrap();
    *storage = Some(Box::new((railway, Vec::<u8>::new())));

    Ok(())
}

fn populator(app: &mut Application, _: MutatorIndex, node_key: NodeKey, xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let xml_node = &app.xml_tree[xml_node_key];
    let mutator_index = xml_node.factory.get().unwrap();
    let mutator = &app.mutators[usize::from(mutator_index)];
    let tag = mutator.xml_tag.clone().unwrap();

    let (content_axis, layout_mode) = parse_tag(app, node_key, tag.deref())?;
    let content_gap = app.attr(node_key, "gap", Some("0".into()))?.as_pixels()?;
    let margin = app.attr(node_key, "margin", Some("0".into()))?.as_pixels()?;
    let radius = app.attr(node_key, "border-radius", Some("0".into()))?.as_pixels()?;

    if let Ok(qa1_callback) = app.attr(node_key, "on-quick-action", None) {
        let qa1_callback = qa1_callback.as_str()?;
        if !app.callbacks.contains_key(&qa1_callback) {
            return Err(error!("Unknown callback: {}", qa1_callback));
        }
    }

    app.view[node_key].margin = Margin::quad(margin + radius);
    app.view[node_key].layout_config.set_content_axis(content_axis);
    app.view[node_key].layout_config.set_content_gap(content_gap);
    app.view[node_key].layout_config.set_layout_mode(layout_mode);
    app.invalidate_layout();

    if let Ok(style) = app.attr(node_key, "style", None) {
        let style_name = style.as_str()?;
        let color = app.theme.get(style_name.deref()).unwrap().background;
        app.view[node_key].background = PixelSource::SolidColor(color);
    }

    let to_generate = if let Ok(result) = app.attr(node_key, "for", None) {
        let store_path = app.attr(node_key, "in", None)?.as_str()?;
        if !store_path.contains(':') {
            return Err(error!("<{} for=... in=...> - missing colon in \"in\"", tag.deref()));
        }

        result.as_str()?;
        app.state_masks.insert(node_key, generator);

        let (store, masker_key) = store_path.split_once(':').unwrap();
        let mut path_hash = Hasher::default();
        let array = app.state_lookup(node_key, store, masker_key, &mut path_hash)?;
        let len = match array.as_array() {
            Some(vector) => vector.len(),
            None => return Err(error!("Generator: {}:{} is not an array", store, masker_key)),
        };
        app.subscribe_to_state(node_key, path_hash.finish());

        Some(len)
    } else if let Ok(_) = app.attr(node_key, "in", None) {
        app.attr(node_key, "for", None)?;
        unreachable!()
    } else {
        None
    };

    for_each_child!(app.xml_tree, xml_node_key, xml_child, {
        let xml_node_index = Some(xml_child.index()).into();
        let add_child = |app: &mut Application| -> Result<_, _> {
            let child_node = app.view.create();
            app.view.append_children(child_node, node_key);
            app.view[child_node].xml_node_index = xml_node_index;
            app.view[child_node].factory = app.xml_tree[xml_child].factory;

            app.populate(child_node, xml_child)
        };

        if let Some(to_generate) = to_generate {
            if app.xml_tree.is_only_child(xml_child) {
                for _ in 0..to_generate {
                    add_child(app)?;
                }
            } else {
                return Err(error!("Generators cannot have more than one XML child"));
            }
        } else {
            add_child(app)?;
        }
    });

    Ok(())
}

fn resizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    if app.debug.skip_container_borders {
        return Ok(());
    }

    let has_style = app.attr(node_key, "style", None).is_ok();
    let border_width = app.attr(node_key, "border-width", None);
    if has_style || border_width.is_ok() {
        let margin = app.attr(node_key, "margin", Some("0".into()))?.as_f32()?;
        let radius = app.attr(node_key, "border-radius", Some("0".into()))?.as_f32()?;

        let mut parent_style = DEFAULT_STYLE.into();
        let mut current = node_key;
        while let Some(parent) = app.view.parent(current) {
            if let Ok(style) = app.attr(parent, "style", None) {
                parent_style = style.as_str()?.clone();
                break;
            } else {
                current = parent;
            }
        }

        let theme = app.theme.get(parent_style.deref()).unwrap();
        let size = app.view[node_key].size;
        let (w, h) = (size.w.to_num(), size.h.to_num());
        let couple = Couple::new(w as f32, h as f32);

        let ext = theme.background;
        let ext_rg = Couple::new((ext.r as f32) / 255.0, (ext.g as f32) / 255.0);
        let ext_ba = Couple::new((ext.b as f32) / 255.0, (ext.a as f32) / 255.0);

        let border = match app.attr(node_key, "style", None) {
            Ok(style) => app.theme.get(style.as_str()?.deref()).unwrap(),
            Err(_) => theme,
        }.outline;

        let border_rg = Couple::new((border.r as f32) / 255.0, (border.g as f32) / 255.0);
        let border_ba = Couple::new((border.b as f32) / 255.0, (border.a as f32) / 255.0);

        let border_width = match border_width {
            Ok(sfr) => Couple::new(sfr.as_f32()?, 0.0),
            Err(_) => C_ZERO,
        };

        let (railway, mask): &mut (R, Vec<u8>) = get_storage(&mut app.storage, m).unwrap();
        railway.set_argument("size", couple).unwrap();
        railway.set_argument("margin-radius", Couple::new(margin, radius)).unwrap();
        railway.set_argument("border-width", border_width).unwrap();
        railway.set_argument("border-rg", border_rg).unwrap();
        railway.set_argument("border-ba", border_ba).unwrap();
        railway.set_argument("ext-rg", ext_rg).unwrap();
        railway.set_argument("ext-ba", ext_ba).unwrap();
        railway.compute().unwrap();

        app.view[node_key].layout_config.set_dirty(true);
        app.view[node_key].foreground = {
            let length = w * h;
            let mut canvas: Vec<u8> = Vec::with_capacity(length * 4);
            canvas.resize(length * 4, 0);
            mask.resize(length, 0);
            railway.render(canvas.as_rgba_mut(), mask, w, h, w, 4, true).unwrap();

            let canvas = canvas.into_boxed_slice();
            PixelSource::Texture(Box::new(RgbaPixelBuffer::new(canvas, w, h)))
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
    if let UserInputEvent::WheelY(wheel_delta) = event {
        let (axis, current_scroll, max_scroll) = get_scroll(app, node_key);
        if max_scroll.is_none() {
            return Ok(false);
        }

        let current_scroll = current_scroll.unwrap_or(SignedPixels::ZERO);
        let max_scroll = max_scroll.unwrap().to_num::<SignedPixels>();

        let mut candidate = *wheel_delta;

        let new_scroll = current_scroll - candidate;
        if new_scroll > max_scroll {
            candidate = current_scroll - max_scroll;
        } else if new_scroll < SignedPixels::ZERO {
            candidate = current_scroll;
        }

        app.view[node_key].layout_config.set_dirty(true);
        scroll(app, node_key, axis, candidate);

        Ok(true)
    } else if let UserInputEvent::QuickAction1 = event {
        if let Ok(callback) = app.attr(node_key, "on-quick-action", None) {
            let callback = app.callbacks.get(&callback.as_str()?).unwrap();

            callback(app, node_key).map(|_| true)
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

fn generator<'a>(
    app: &'a mut Application,
    masker: NodeKey,
    node: NodeKey,
    store: &str,
    key: &str,
    path_hash: &mut Hasher,
) -> Result<&'a mut StateValue, Error> {
    let expected_store = app.attr(masker, "for", None)?.as_str()?;
    if store == expected_store.deref() {
        let mut child = node;
        loop {
            let parent = app.view.parent(child).unwrap();
            match parent == masker {
                true => break,
                false => child = parent,
            }
        }

        let index = app.view.child_num(child).unwrap();
        let store_path = app.attr(masker, "in", None)?.as_str()?;
        let (store, masker_key) = store_path.split_once(':').unwrap();
        let mut current = app.state_lookup(masker, store, masker_key, path_hash)?;

        path_hash.write_usize(index);
        current = &mut current.as_array_mut().unwrap()[index];
        for path_step in path_steps(key) {
            let option = match path_step {
                StatePathStep::Index(index) => {
                    path_hash.write_usize(index);
                    current.get_mut(index)
                },
                StatePathStep::Key(key) => {
                    path_hash.write(key.as_bytes());
                    current.get_mut(key)
                },
            };

            current = match option {
                Some(value) => value,
                None => return Err(error!("Invalid state key: {}", key)),
            }
        }

        Ok(current)
    } else {
        app.state_lookup(masker, store, key, path_hash)
    }
}

macro_rules! container {
    ($v:ident, $h:ident, $vtag:literal, $htag:literal $(, $arg:literal)?) => {
        const $v: Mutator = Mutator {
            name: cheap_string(stringify!($v)),
            xml_tag: Some(cheap_string($vtag)),
            xml_attr_set: Some(&["for", "in", "style", "margin", "border-width", "border-radius", "gap", "on-quick-action", $($arg)*]),
            xml_accepts_children: true,
            handlers: Handlers {
                initializer,
                populator,
                resizer,
                user_input_handler,
                ..DEFAULT_HANDLERS
            },
        };

        const $h: Mutator = Mutator {
            name: cheap_string(stringify!($h)),
            xml_tag: Some(cheap_string($htag)),
            xml_attr_set: Some(&["for", "in", "style", "margin", "border-width", "border-radius", "gap", "on-quick-action", $($arg)*]),
            xml_accepts_children: true,
            handlers: Handlers {
                initializer,
                populator,
                resizer,
                user_input_handler,
                ..DEFAULT_HANDLERS
            },
        };
    }
}

container!(HC_CHUNKS_MUTATOR, VC_CHUNKS_MUTATOR, "h-chunks", "v-chunks", "row");
container!(HC_FIXED_MUTATOR, VC_FIXED_MUTATOR, "h-fixed", "v-fixed", "length");
container!(HC_RATIO_MUTATOR, VC_RATIO_MUTATOR, "h-ratio", "v-ratio", "ratio");
container!(HC_WRAP_MUTATOR, VC_WRAP_MUTATOR, "h-wrap", "v-wrap");
container!(HC_REM_MUTATOR, VC_REM_MUTATOR, "h-rem", "v-rem", "weight");

pub const CONTAINERS: [Mutator; 10] = [
    HC_CHUNKS_MUTATOR, VC_CHUNKS_MUTATOR,
    HC_FIXED_MUTATOR, VC_FIXED_MUTATOR,
    HC_RATIO_MUTATOR, VC_RATIO_MUTATOR,
    HC_WRAP_MUTATOR, VC_WRAP_MUTATOR,
    HC_REM_MUTATOR, VC_REM_MUTATOR,
];
