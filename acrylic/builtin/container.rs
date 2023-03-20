use crate::core::app::{Application, Mutator};
use crate::core::event::Event;
use crate::core::node::NodeKey;
use crate::core::xml::tag;
use crate::core::visual::{Ratio, Margin, Axis, LayoutMode, PixelSource};
use crate::core::state::{StateValue, path_steps};
use crate::core::style::DEFAULT_STYLE;
use crate::core::for_each_child;
use oakwood::NodeKey as _;
use crate::{Error, error, Hasher};
use core::{ops::Deref, hash::Hasher as _};

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

fn container(app: &mut Application, event: Event) -> Result<(), Error> {
    match event {
        Event::Populate { node_key, xml_node_key } => {
            let xml_node = &app.xml_tree[xml_node_key];
            let mutator_index = xml_node.factory.get().unwrap();
            let mutator = &app.mutators[usize::from(mutator_index)];
            let tag = mutator.xml_tag.clone().unwrap();

            let (content_axis, layout_mode) = parse_tag(app, node_key, tag.deref())?;
            let content_gap = app.attr(node_key, "gap", Some("0".into()))?.as_pixels()?;
            let margin = app.attr(node_key, "margin", Some("0".into()))?.as_pixels()?;

            app.view[node_key].margin = Margin::quad(margin);
            app.view[node_key].layout_config.set_content_axis(content_axis);
            app.view[node_key].layout_config.set_content_gap(content_gap);
            app.view[node_key].layout_config.set_layout_mode(layout_mode);
            app.must_check_layout = true;

            if let Ok(style) = app.attr(node_key, "style", None) {
                let style_name = style.as_str()?;
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
                log::info!("style override: from {} to {}", parent_style, style_name);
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

                    app.handle(child_node, Event::Populate {
                        node_key: child_node,
                        xml_node_key: xml_child,
                    })
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
        },
        Event::Resized { .. } => Ok(()),
        _ => Err(error!("Unexpected event: {:?}", event)),
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
            path_hash.write(path_step.as_bytes());
            current = match current.get_mut(path_step) {
                Some(value) => value,
                None => return Err(error!("Invalid state key: {}", key)),
            }
        }

        Ok(current)
    } else {
        app.state_lookup(node, store, key, path_hash)
    }
}

fn inflate(app: &mut Application, event: Event) -> Result<(), Error> {
    match event {
        Event::Populate { node_key, .. } => {
            let layout_mode = LayoutMode::Remaining(Ratio::from_num(1));
            app.view[node_key].layout_config.set_layout_mode(layout_mode);
            app.must_check_layout = true;

            Ok(())
        },
        Event::Resized { .. } => Ok(()),
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}

macro_rules! container {
    ($v:ident, $h:ident, $vtag:literal, $htag:literal $(, $arg:literal)?) => {
        const $v: Mutator = Mutator {
            xml_tag: Some(tag($vtag)),
            xml_attr_set: Some(&["for", "in", "style", "margin", "gap", $($arg)*]),
            xml_accepts_children: true,
            handler: container,
            storage: None,
        };

        const $h: Mutator = Mutator {
            xml_tag: Some(tag($htag)),
            xml_attr_set: Some(&["for", "in", "style", "margin", "gap", $($arg)*]),
            xml_accepts_children: true,
            handler: container,
            storage: None,
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

pub const INF_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("inflate")),
    xml_attr_set: Some(&[]),
    xml_accepts_children: false,
    handler: inflate,
    storage: None,
};
