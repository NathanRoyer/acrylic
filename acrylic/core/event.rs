use super::xml::XmlNodeKey;
use super::node::NodeKey;

pub type InputEvent = usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Handler should modify the state of the application
    /// and/or the properties of the view node.
    UserInput {
        node_key: NodeKey,
        event: InputEvent,
    },

    /// Handler can modify the properties of the view node
    /// based on the actual pixel size of the node.
    Resized {
        node_key: NodeKey,
    },

    /// Handler should set the properties of a view Node
    /// based on those of the XmlNode and the current
    /// state of the application. The view Node is
    /// expected to have the default configuration upon.
    /// firing of this event, except `xml_node_index`
    /// and `factory`, which will be set appropriately.
    /// Handler is allowed to delete the view node.
    Populate {
        node_key: NodeKey,
        xml_node_key: XmlNodeKey,
    },

    /// Handler can process the asset bytes.
    AssetLoaded {
        node_key: NodeKey,
    },
}