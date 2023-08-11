use crate::core::xml::{XmlNodeKey, XmlTagParameters, AttributeValueType, parse_xml_tree};
use crate::{Box, HashMap, ArcStr, Error, ro_string};
use crate::core::app::Application;
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::core::node::{NodeKey, Mutator, MutatorIndex, get_storage};
use oakwood::NodeKey as _;

const FILE: usize = 0;

pub const IMPORT_MUTATOR: Mutator = Mutator {
    name: ro_string!("ImportMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("import"),
        attr_set: &[ ("file", AttributeValueType::Other, None) ],
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

type SubLayouts = HashMap<ArcStr, XmlNodeKey>;

fn initializer(app: &mut Application, m: MutatorIndex) -> Result<(), Error> {
    let storage = &mut app.mutators[usize::from(m)].storage;
    assert!(storage.is_none());

    *storage = Some(Box::new(SubLayouts::new()));

    Ok(())
}

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _xml_node_key: XmlNodeKey) -> Result<(), Error> {
    let layout_asset = app.attr(node_key, FILE)?;
    app.request(&layout_asset, node_key, true)
}

fn parser(app: &mut Application, m: MutatorIndex, _node_key: NodeKey, asset: &ArcStr, bytes: Box<[u8]>) -> Result<(), Error> {
    let mut xml_tags = HashMap::<str, (&XmlTagParameters, MutatorIndex)>::new();
    for i in 0..app.mutators.len() {
        if let Some(params) = &app.mutators[i].xml_params {
            xml_tags.insert_ref(&params.tag_name.clone(), (params, i.into()));
        }
    }

    let replacement = parse_xml_tree(
        xml_tags,
        &app.mutators,
        &mut app.xml_tree,
        &bytes,
    )?;

    let storage: &mut SubLayouts = get_storage(&mut app.mutators, m).unwrap();
    storage.insert(asset.clone(), replacement);

    Ok(())
}

fn finalizer(app: &mut Application, m: MutatorIndex, node_key: NodeKey) -> Result<(), Error> {
    let file: ArcStr = app.attr(node_key, FILE)?;

    let replacement = {
        let storage: &mut SubLayouts = get_storage(&mut app.mutators, m).unwrap();
        *storage.get(&file).unwrap()
    };

    app.view.reset(node_key);
    app.view[node_key].xml_node_index = Some(replacement.index()).into();
    app.view[node_key].factory = app.xml_tree[replacement].factory;

    app.populate(node_key, replacement)
}
