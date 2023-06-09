use crate::core::app::{Application, Mutator, MutatorIndex, get_storage};
use crate::core::event::{Handlers, DEFAULT_HANDLERS, UserInputEvent};
use crate::core::node::NodeKey;
use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType};
use crate::core::visual::{Pixels, SignedPixels, Margin, Axis, LayoutMode, PixelSource, RgbaPixelBuffer, PixelBuffer};
use crate::core::state::StateValue;
use crate::core::style::DEFAULT_STYLE;
use crate::core::{for_each_child, rgb::FromSlice};
use crate::core::layout::{get_scroll, scroll};
use crate::{Error, error, Hasher, Box, Vec, CheapString, cheap_string};
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
        "fixed" => LayoutMode::Fixed(app.attr(node, LENGTH)?),
        "chunks" => LayoutMode::Chunks(app.attr(node, ROW)?),
        "ratio" => LayoutMode::AspectRatio(app.attr(node, RATIO)?),
        "rem" => LayoutMode::Remaining(app.attr(node, WEIGHT)?),
        "wrap" => LayoutMode::WrapContent,
        _ => unreachable!(),
    };

    Ok((axis, mode))
}

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.mutators[usize::from(m)].storage;
    assert!(storage.is_none());

    let railway = R::parse(include_bytes!(concat!(env!("OUT_DIR"), "/container.rwy"))).unwrap();
    *storage = Some(Box::new((railway, Vec::<u8>::new())));

    Ok(())
}

fn populator(app: &mut Application, _: MutatorIndex, node_key: NodeKey, xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let    for_attr: Option<CheapString> = app.attr(node_key,             FOR)?;
    let     in_attr: Option<CheapString> = app.attr(node_key,              IN)?;
    let  style_attr: Option<CheapString> = app.attr(node_key,           STYLE)?;
    let qa_callback: Option<CheapString> = app.attr(node_key, ON_QUICK_ACTION)?;
    let content_gap: Pixels              = app.attr(node_key,             GAP)?;
    let margin_attr: Pixels              = app.attr(node_key,          MARGIN)?;
    let radius_attr: Pixels              = app.attr(node_key,   BORDER_RADIUS)?;

    let xml_node = &app.xml_tree[xml_node_key];
    let mutator_index = xml_node.factory.get().unwrap();
    let mutator = &app.mutators[usize::from(mutator_index)];
    let tag = mutator.xml_params.as_ref().unwrap().tag_name.clone();

    let (content_axis, layout_mode) = parse_tag(app, node_key, tag.deref())?;

    if let Some(qa_callback) = qa_callback {
        if !app.callbacks.contains_key(&qa_callback) {
            return Err(error!("Unknown callback: {}", qa_callback));
        }
    }

    app.view[node_key].margin = Margin::quad(margin_attr + radius_attr);
    app.view[node_key].layout_config.set_content_axis(content_axis);
    app.view[node_key].layout_config.set_content_gap(content_gap);
    app.view[node_key].layout_config.set_layout_mode(layout_mode);
    app.invalidate_layout();

    if let Some(style) = style_attr {
        let color = app.theme.get(&style).unwrap().background;
        app.view[node_key].background = PixelSource::SolidColor(color);
    }

    let to_generate = if for_attr.is_some() {
        let namespace_path = match in_attr {
            Some(cs) => cs,
            None => return Err(error!("<{} for=... in=...> - missing \"in\" attribute", tag.deref())),
        };

        if !namespace_path.contains(':') {
            return Err(error!("<{} for=... in=...> - missing colon in \"in\"", tag.deref()));
        }

        app.state_masks.insert(node_key, generator);

        let (namespace, masker_key) = namespace_path.split_once(':').unwrap();
        let mut path_hash = Hasher::default();
        let len = match app.state_lookup(node_key, namespace, masker_key, &mut path_hash)? {
            StateValue::Array(vector) => vector.len(),
            _ => return Err(error!("Generator: {}:{} is not an array", namespace, masker_key)),
        };
        app.subscribe_to_state(node_key, path_hash.finish());

        Some(len)
    } else if in_attr.is_some() {
        return Err(error!("<{} for=... in=...> - missing \"for\" attribute", tag.deref()));
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

    let        style: Option<CheapString> = app.attr(node_key,        STYLE)?;
    let border_width: Option<     Pixels> = app.attr(node_key, BORDER_WIDTH)?;

    if style.is_some() || border_width.is_some() {
        let margin: Pixels = app.attr(node_key,        MARGIN)?;
        let radius: Pixels = app.attr(node_key, BORDER_RADIUS)?;

        let mut parent_style = DEFAULT_STYLE.into();
        let mut current = node_key;
        while let Some(parent) = app.view.parent(current) {
            let style: Option<CheapString> = app.attr(parent, STYLE)?;
            if let Some(style) = style {
                parent_style = style.clone();
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

        let border = match style {
            Some(style) => app.theme.get(style.deref()).unwrap(),
            None => theme,
        }.outline;

        let border_rg = Couple::new((border.r as f32) / 255.0, (border.g as f32) / 255.0);
        let border_ba = Couple::new((border.b as f32) / 255.0, (border.a as f32) / 255.0);

        let border_width = match border_width {
            Some(sfr) => Couple::new(sfr.to_num(), 0.0),
            None => C_ZERO,
        };

        let (railway, mask): &mut (R, Vec<u8>) = get_storage(&mut app.mutators, m).unwrap();
        railway.set_argument("size", couple).unwrap();
        railway.set_argument("margin-radius", Couple::new(margin.to_num(), radius.to_num())).unwrap();
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
        let qa_callback: Option<CheapString> = app.attr(node_key, ON_QUICK_ACTION)?;
        if let Some(qa_callback) = qa_callback {
            let callback = app.callbacks.get(&qa_callback).unwrap();
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
    namespace: &str,
    key: &str,
    path_hash: &mut Hasher,
) -> Result<&'a mut StateValue, Error> {
    let expected_namespace: Option<CheapString> = app.attr(masker, FOR)?;
    let expected_namespace = expected_namespace.unwrap();

    if namespace == &*expected_namespace {
        let namespace_path: Option<CheapString> = app.attr(masker,  IN)?;
        let namespace_path = namespace_path.unwrap();

        let mut child = node;
        loop {
            let parent = app.view.parent(child).unwrap();
            match parent == masker {
                true => break,
                false => child = parent,
            }
        }

        let index = app.view.child_num(child).unwrap();
        let (namespace, masker_key) = namespace_path.split_once(':').unwrap();
        let array = match app.state_lookup(masker, namespace, masker_key, path_hash)? {
            StateValue::Array(array) => array,
            _ => return Err(error!("Generator: {}:{} is not an array", namespace, masker_key)),
        };

        path_hash.write_usize(index);
        array[index].get_mut(key, path_hash)
    } else {
        app.state_lookup(masker, namespace, key, path_hash)
    }
}

// common attributes
const             FOR: usize = 0;
const              IN: usize = 1;
const           STYLE: usize = 2;
const          MARGIN: usize = 3;
const    BORDER_WIDTH: usize = 4;
const   BORDER_RADIUS: usize = 5;
const             GAP: usize = 6;
const ON_QUICK_ACTION: usize = 7;

// specific
const             ROW: usize = 8;
const          LENGTH: usize = 8;
const           RATIO: usize = 8;
const          WEIGHT: usize = 8;

macro_rules! container {
    ($name:ident, $tag:literal $(, $arg:expr)?) => {
        const $name: Mutator = Mutator {
            name: cheap_string(stringify!($name)),
            xml_params: Some(XmlTagParameters {
                tag_name: cheap_string($tag),
                attr_set: &[
                    ("for", AttributeValueType::OptOther, None),
                    ("in", AttributeValueType::OptOther, None),
                    ("style", AttributeValueType::OptOther, None),
                    ("margin", AttributeValueType::Pixels, Some("0")),
                    ("border-width", AttributeValueType::OptPixels, None),
                    ("border-radius", AttributeValueType::Pixels, Some("0")),
                    ("gap", AttributeValueType::Pixels, Some("0")),
                    ("on-quick-action", AttributeValueType::OptOther, None),
                    $($arg)*
                ],
                accepts_children: true,
            }),
            handlers: Handlers {
                initializer,
                populator,
                resizer,
                user_input_handler,
                ..DEFAULT_HANDLERS
            },
            storage: None,
        };
    };
    ($v:ident, $h:ident, $vtag:literal, $htag:literal $(, $arg:expr)?) => {
        container!($v, $vtag $(, $arg)*);
        container!($h, $htag $(, $arg)*);
    };
}

container!(HC_CHUNKS_MUTATOR, VC_CHUNKS_MUTATOR, "h-chunks", "v-chunks", ("row", AttributeValueType::Pixels, None));
container!(HC_FIXED_MUTATOR, VC_FIXED_MUTATOR, "h-fixed", "v-fixed", ("length", AttributeValueType::Pixels, None));
container!(HC_RATIO_MUTATOR, VC_RATIO_MUTATOR, "h-ratio", "v-ratio", ("ratio", AttributeValueType::Ratio, None));
container!(HC_WRAP_MUTATOR, VC_WRAP_MUTATOR, "h-wrap", "v-wrap");
container!(HC_REM_MUTATOR, VC_REM_MUTATOR, "h-rem", "v-rem", ("weight", AttributeValueType::Ratio, Some("1")));

pub const CONTAINERS: [Mutator; 10] = [
    HC_CHUNKS_MUTATOR, VC_CHUNKS_MUTATOR,
    HC_FIXED_MUTATOR, VC_FIXED_MUTATOR,
    HC_RATIO_MUTATOR, VC_RATIO_MUTATOR,
    HC_WRAP_MUTATOR, VC_WRAP_MUTATOR,
    HC_REM_MUTATOR, VC_REM_MUTATOR,
];
