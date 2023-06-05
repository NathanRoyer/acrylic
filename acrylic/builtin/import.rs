//! Imported XML Layouts
//!
//! # List of tags
//!
//! ## `import`
//!
//! Imports another XML layout file into the current one.
//!
//! Special Attribute: `file` (name of the asset, no default)

use crate::core::xml::{tag, XmlNodeKey, parse_xml_tree};
use crate::{Box, HashMap, CheapString, Error, error};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::event::Event;
use oakwood::NodeKey as _;

pub const IMPORT_MUTATOR: Mutator = Mutator {
    xml_tag: Some(tag("import")),
    xml_attr_set: Some(&[ "file" ]),
    xml_accepts_children: false,
    handler: import,
};

type SubLayouts = HashMap<CheapString, XmlNodeKey>;

fn import(app: &mut Application, m: MutatorIndex, event: Event) -> Result<(), Error> {
    match event {
        Event::Initialize => {
            let storage = &mut app.storage[usize::from(m)];
            assert!(storage.is_none());

            *storage = Some(Box::new(SubLayouts::new()));

            Ok(())
        },
        Event::Populate { node_key, .. } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;
            app.request(file, node_key, true)
        },
        Event::ParseAsset { asset, bytes, .. } => {
            let replacement = parse_xml_tree(
                &mut app.mutators,
                &mut app.xml_tree,
                &bytes,
            )?;

            let storage = app.storage[usize::from(m)].as_mut().unwrap();
            let storage: &mut SubLayouts = storage.downcast_mut().unwrap();
            storage.insert(asset, replacement);

            Ok(())
        },
        Event::AssetLoaded { node_key } => {
            let file = app.attr(node_key, "file", None)?.as_str()?;

            let replacement = {
                let storage = app.storage[usize::from(m)].as_ref().unwrap();
                let storage: &SubLayouts = storage.downcast_ref().unwrap();
                storage[&file]
            };

            app.view.reset(node_key);
            app.view[node_key].xml_node_index = Some(replacement.index()).into();
            app.view[node_key].factory = app.xml_tree[replacement].factory;

            app.handle(node_key, Event::Populate {
                node_key,
                xml_node_key: replacement,
            })?.ok_or_else(|| error!())
        },
        // Event::Resized { .. } => Ok(()),
        _ => Err(error!("Unexpected event: {:?}", event)),
    }
}
