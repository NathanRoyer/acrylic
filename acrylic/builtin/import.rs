use crate::core::xml::{XmlNodeKey, parse_xml_tree};
use crate::{Box, HashMap, CheapString, Error, cheap_string};
use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::core::node::NodeKey;
use oakwood::NodeKey as _;

pub const IMPORT_MUTATOR: Mutator = Mutator {
    name: cheap_string("ImportMutator"),
    xml_tag: Some(cheap_string("import")),
    xml_attr_set: Some(&[ "file" ]),
    xml_accepts_children: false,
    handlers: Handlers {
        initializer,
        populator,
        parser,
        finalizer,
        ..DEFAULT_HANDLERS
    },
};

type SubLayouts = HashMap<CheapString, XmlNodeKey>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.storage[usize::from(m)];
    assert!(storage.is_none());

    *storage = Some(Box::new(SubLayouts::new()));

    Ok(())
}

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;
    app.request(file, node_key, true)
}

fn parser(app: &mut Application, m: MutatorIndex, _node_key: NodeKey, asset: CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
    let replacement = parse_xml_tree(
        &mut app.mutators,
        &mut app.xml_tree,
        &bytes,
    )?;

    let storage = app.storage[usize::from(m)].as_mut().unwrap();
    let storage: &mut SubLayouts = storage.downcast_mut().unwrap();
    storage.insert(asset, replacement);

    Ok(())
}

fn finalizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;

    let replacement = {
        let storage = app.storage[usize::from(m)].as_ref().unwrap();
        let storage: &SubLayouts = storage.downcast_ref().unwrap();
        storage[&file]
    };

    app.view.reset(node_key);
    app.view[node_key].xml_node_index = Some(replacement.index()).into();
    app.view[node_key].factory = app.xml_tree[replacement].factory;

    app.populate(node_key, replacement)
}
