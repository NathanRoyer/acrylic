use crate::core::xml::{XmlNodeKey, XmlTagParameters, parse_xml_tree};
use crate::{Box, HashMap, CheapString, Error, cheap_string};
use crate::core::app::{Application, Mutator, MutatorIndex, get_storage};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::core::node::NodeKey;
use oakwood::NodeKey as _;

pub const IMPORT_MUTATOR: Mutator = Mutator {
    name: cheap_string("ImportMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: cheap_string("import"),
        attr_set: &[ "file" ],
        accepts_children: false,
    }),
    handlers: Handlers {
        initializer,
        populator,
        parser,
        finalizer,
        ..DEFAULT_HANDLERS
    },
    storage: None,
};

type SubLayouts = HashMap<CheapString, XmlNodeKey>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.mutators[usize::from(m)].storage;
    assert!(storage.is_none());

    *storage = Some(Box::new(SubLayouts::new()));

    Ok(())
}

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;
    app.request(&file, node_key, true)
}

fn parser(app: &mut Application, m: MutatorIndex, _node_key: NodeKey, asset: &CheapString, bytes: Box<[u8]>) -> Result<(), Error> {
    let replacement = parse_xml_tree(
        &mut app.mutators,
        &mut app.xml_tree,
        &bytes,
    )?;

    let storage: &mut SubLayouts = get_storage(&mut app.mutators, m).unwrap();
    storage.insert(asset.clone(), replacement);

    Ok(())
}

fn finalizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let file = app.attr(node_key, "file", None)?.as_str()?;

    let replacement = {
        let storage: &mut SubLayouts = get_storage(&mut app.mutators, m).unwrap();
        *storage.get(&file).unwrap()
    };

    app.view.reset(node_key);
    app.view[node_key].xml_node_index = Some(replacement.index()).into();
    app.view[node_key].factory = app.xml_tree[replacement].factory;

    app.populate(node_key, replacement)
}
