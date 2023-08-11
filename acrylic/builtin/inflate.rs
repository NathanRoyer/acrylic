use crate::core::app::Application;
use crate::core::event::{Handlers, DEFAULT_HANDLERS};
use crate::core::node::{NodeKey, Mutator, MutatorIndex};
use crate::core::xml::{XmlNodeKey, XmlTagParameters};
use crate::core::visual::{Ratio, LayoutMode};
use crate::{Error, ro_string};

fn populator(app: &mut Application, _m: MutatorIndex, node_key: NodeKey, _: XmlNodeKey) -> Result<(), Error> {
    let layout_mode = LayoutMode::Remaining(Ratio::from_num(1));
    app.view[node_key].layout_config.set_layout_mode(layout_mode);
    app.invalidate_layout();

    Ok(())
}

pub const INFLATE_MUTATOR: Mutator = Mutator {
    name: ro_string!("InflateMutator"),
    xml_params: Some(XmlTagParameters {
        tag_name: ro_string!("inflate"),
        attr_set: &[],
        accepts_children: false,
    }),
    handlers: Handlers {
        populator,
        ..DEFAULT_HANDLERS
    },
    storage: None,
};