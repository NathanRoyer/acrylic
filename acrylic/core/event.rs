//! Event definitions

use super::app::Application;
use super::xml::XmlNodeKey;
use super::node::{NodeKey, MutatorIndex};
use super::visual::{Direction, Ratio, SignedPixels};
use crate::{Box, ArcStr, Error, error};

#[cfg(doc)]
use super::node::Mutator;

/// Initializes a [`Mutator`], and especially its `storage` field
pub type Initializer = fn(
    app: &mut Application,
    m: MutatorIndex,
) -> Result<(), Error>;

/// Parses an asset's bytes and optionally stores the result in the [`Mutator`]'s storage
///
/// # Arguments
///
/// - `node_key`: The first node that requested this asset
/// - `asset`: The asset's file path
/// - `bytes`: The asset's raw content
pub type Parser = fn(
    app: &mut Application,
    m: MutatorIndex,
    node_key: NodeKey,
    asset: &ArcStr,
    bytes: Box<[u8]>,
) -> Result<(), Error>;

/// Sets-up a node's fields (and optionally, its children)
///
/// # Arguments
///
/// - `node_key`: The just-created node which should be initialized
/// - `xml_node_key`: a key of the XML Node Tree, which can be used
/// to inspect the XML tree and get attribute values
/// (see [`Application::attr`] for this in particular).
pub type Populator = fn(
    app: &mut Application,
    m: MutatorIndex,
    node_key: NodeKey,
    xml_node_key: XmlNodeKey,
) -> Result<(), Error>;

/// Finishes setting up a node's field (and children) after a requested asset as been loaded
///
/// # Arguments
///
/// - `node_key`: The node which requested an asset
pub type Finalizer = fn(
    app: &mut Application,
    m: MutatorIndex,
    node_key: NodeKey,
) -> Result<(), Error>;

/// Updates a node's textures after it's been resized by the layout
///
/// # Arguments
///
/// - `node_key`: The node which was resized
pub type Resizer = fn(
    app: &mut Application,
    m: MutatorIndex,
    node_key: NodeKey,
) -> Result<(), Error>;

/// Processes user input
///
/// # Arguments
///
/// - `node_key`: The node which was selected for handling
/// - `target`: The node with which the user interacted
/// - `event`: The type and details of this user input event
///
/// # Return value
///
/// `true` if the event was handled and shouldn't be propagated.
pub type UserInputHandler = fn(
    app: &mut Application,
    m: MutatorIndex,
    node_key: NodeKey,
    target: NodeKey,
    event: &UserInputEvent,
) -> Result<bool, Error>;

/// Dispatch Table for [`Mutator`]s
#[derive(Copy, Clone)]
pub struct Handlers {
    pub initializer: Initializer,
    pub parser: Parser,
    pub populator: Populator,
    pub finalizer: Finalizer,
    pub resizer: Resizer,
    pub user_input_handler: UserInputHandler,
}

fn initializer(_app: &mut Application, _m: MutatorIndex) -> Result<(), Error> {
    Ok(())
}

fn parser(app: &mut Application, m: MutatorIndex, _: NodeKey, _: &ArcStr, _: Box<[u8]>) -> Result<(), Error> {
    Err(error!("{}: parser is unimplemented", app.mutators[usize::from(m)].name))
}

fn populator(app: &mut Application, m: MutatorIndex, _: NodeKey, _: XmlNodeKey) -> Result<(), Error> {
    Err(error!("{}: populator is unimplemented", app.mutators[usize::from(m)].name))
}

fn finalizer(app: &mut Application, m: MutatorIndex, _: NodeKey) -> Result<(), Error> {
    Err(error!("{}: finalizer is unimplemented", app.mutators[usize::from(m)].name))
}

fn resizer(_app: &mut Application, _m: MutatorIndex, _: NodeKey) -> Result<(), Error> {
    Ok(())
}

fn user_input_handler(_: &mut Application, _: MutatorIndex, _: NodeKey, _: NodeKey, _: &UserInputEvent) -> Result<bool, Error> {
    Ok(false)
}

/// Default handlers (see detailed doc)
///
/// - `initializer`: does nothing
/// - `resizer`: does nothing
/// - `user_input_handler`: does nothing, returns false
/// - `parser`: returns an error
/// - `populator`: returns an error
/// - `finalizer`: returns an error
pub const DEFAULT_HANDLERS: Handlers = Handlers {
    initializer,
    parser,
    populator,
    finalizer,
    resizer,
    user_input_handler,
};

/// Events resulting from user interaction 
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserInputEvent<'a> {
    QuickAction1,
    QuickAction2,
    QuickAction3,
    QuickAction4,
    QuickAction5,
    QuickAction6,
    /// Zoom Variant 1
    Factor1(Ratio),
    /// Zoom Variant 2
    Factor2(Ratio),
    /// Pan Variant 1
    Pan1(usize, usize),
    /// Pan Variant 2
    Pan2(usize, usize),
    /// Horizontal Scroll
    WheelX(SignedPixels),
    /// Vertical Scroll
    WheelY(SignedPixels),
    /// Entirely replace the current content
    TextReplace(&'a str),
    /// Insert some text at the current position
    TextInsert(&'a str),
    /// Delete text, from the current position to an offset;
    /// the offset is a byte offset (todo: make this a char offset);
    /// A value of zero means nothing is deleted.
    TextDelete(isize),
    /// User unselected this node
    ///
    /// Set app.focused to a nodekey to grab focus
    FocusLoss,
    /// Nodes which grabbed the focus
    /// receives this special event:
    DirInput(Direction),
}
