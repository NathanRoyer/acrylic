//! The Node structure

use super::xml::{OptionalXmlNodeIndex, XmlTagParameters};
use super::visual::{PixelSource, NodeConfig, Margin, Size, Position};
use oakwood::{Cookie64, tree, index};
use super::event::Handlers;
use crate::{ArcStr, Box};
use core::any::Any;

#[cfg(doc)]
use super::event::Initializer;

index!(MutatorIndex, OptionalMutatorIndex, u16);

index!(StyleIndex, OptionalStyleIndex, u16);

tree!(NodeTree, Node, NodeKey, NodeIndex, OptionalNodeIndex, Cookie64);

/// A Visual Element
///
/// [`Mutator`]s typically convert XML Tags to one or more nodes.
#[derive(Debug, Default)]
pub struct Node {                                 // bits    div4
    pub config: NodeConfig,                       // 2x4     2
    pub margin: Margin,                           // 4x4     4

    pub size: Size,                               // 2x4     2
    pub position: Position,                       // 2x4     2

    pub background: PixelSource,                  // 2x8     4
    pub foreground: PixelSource,                  // 2x8     4

    pub factory: OptionalMutatorIndex,            // 1x2
    pub style_override: OptionalStyleIndex,       // 1x2     1

    pub xml_node_index: OptionalXmlNodeIndex,     // 1x4     1

    // todo:
    // pub transition: Transition,
}                                                 //         21x4

/// XML Tags & other event handlers are defined as Mutators
pub struct Mutator {
    pub name: ArcStr,
    pub xml_params: Option<XmlTagParameters>,
    pub handlers: Handlers,
    /// Must be None initially; initialize it via an [`Initializer`].
    pub storage: Option<Box<dyn Any>>,
}

/// Utility function for event handlers to get and downcast their storage
pub fn get_storage<T: Any>(mutators: &mut [Mutator], m: MutatorIndex) -> Option<&mut T> {
    mutators[usize::from(m)].storage.as_mut()?.downcast_mut()
}

impl Clone for Mutator {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            xml_params: self.xml_params.clone(),
            handlers: self.handlers.clone(),
            storage: match self.storage.is_some() {
                true => panic!("Tried to Clone Mutator with an initialized storage"),
                false => None,
            },
        }
    }
}
