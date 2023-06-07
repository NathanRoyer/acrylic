use crate::core::app::{Application, Mutator, MutatorIndex};
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::core::node::NodeKey;
use crate::core::xml::XmlNodeKey;
use crate::core::visual::{Ratio, LayoutMode};
use crate::{Error, cheap_string};

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _: XmlNodeKey) -> Result<(), Error> {
    let layout_mode = LayoutMode::Remaining(Ratio::from_num(1));
    app.view[node_key].layout_config.set_layout_mode(layout_mode);
    app.invalidate_layout();

    Ok(())
}

pub const INFLATE_MUTATOR: Mutator = Mutator {
    name: cheap_string("InflateMutator"),
    xml_tag: Some(cheap_string("inflate")),
    xml_attr_set: Some(&[]),
    xml_accepts_children: false,
    handlers: Handlers {
        populator,
        ..DEFAULT_HANDLERS
    },
};