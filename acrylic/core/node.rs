//! The Node structure

use super::app::OptionalMutatorIndex;
use super::xml::OptionalXmlNodeIndex;
use super::visual::{PixelSource, LayoutConfig, Margin, Size, Position};
use oakwood::{Cookie64, tree};

#[cfg(doc)]
use super::app::Mutator;

tree!(NodeTree, Node, NodeKey, NodeIndex, OptionalNodeIndex, Cookie64);

/// A Visual Element
///
/// [`Mutator`]s typically convert XML Tags to one or more nodes.
#[derive(Debug, Default)]
pub struct Node {                                 // bits    div4
    pub layout_config: LayoutConfig,              // 2x4     2
    pub margin: Margin,                           // 4x4     4

    pub size: Size,                               // 2x4     2
    pub position: Position,                       // 2x4     2

    pub background: PixelSource,                  // 2x8     4
    pub foreground: PixelSource,                  // 2x8     4

    pub factory: OptionalMutatorIndex,            // 1x4     1
    pub xml_node_index: OptionalXmlNodeIndex,     // 1x4     1

    // todo:
    // pub transition: Transition,
}                                                 //         21x4
