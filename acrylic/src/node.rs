//! Node, NodePath, RenderCache, Event, LengthPolicy...

use bitflags::bitflags;

use crate::app::Application;
use crate::app::ScratchBuffer;
use crate::flexbox::Cursor;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

use log::info;
use log::error;

use core::any::Any;
use core::fmt::Debug;
use core::mem::swap;

use alloc::string::String;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Nodes specify a policy to be layed out in various ways
/// by the [layout functions](`crate::flexbox::compute_tree`).
#[derive(Debug, Copy, Clone)]
pub enum LengthPolicy {
    // needs two passes in diff-axis config
    // needs one pass in same-axis config
    /// Main length is just enough to contain all children.
    /// Valid for containers only.
    WrapContent,
    /// Main length is a fixed number of pixels.
    Fixed(usize),
    /// Main length is divided in chunks of specified
    /// length (in pixels). The number of chunks is
    /// determined by the contained nodes: there will
    /// be as many chunks as necessary for all children
    /// to fit in.
    /// For this to work, the node must be:
    /// * A vertical container in an vorizontal container, or
    /// * An horizontal container in a vertical container.
    Chunks(usize),
    /// Main length is computed from the cross length
    /// so that the size of the node maintains a certain
    /// aspect ratio.
    AspectRatio(f64),
    /// After neighbors with a different policy are layed
    /// out, nodes with this policy are layed-out so that
    /// they occupy the remaining space in their container.
    /// The `f64` is the relative "weight" of this node:
    /// heavier nodes will get more space, lighter nodes
    /// will get less space. If they all have the same
    /// weight, they will all get the same space.
    Remaining(f64),
}

/// General-purpose axis enumeration
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// General-purpose axis enumeration
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RenderLayer {
    Background,
    Foreground,
}

/// Nodes use this internally to know when to render
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RenderReason {
    None,
    Computation,
    Resized,
}

pub type RenderCache = [Option<Vec<u8>>; 2];

/// This can be used by [`Node`] implementations
/// to offset the boundaries of their original
/// rendering spot.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Margin {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
}

bitflags! {
    /// This is a bit field of events supported
    /// by this toolkit.
    pub struct EventType: u32 {
        const QUICK_ACTION_1 = 0b0000000000000000001;
        const QUICK_ACTION_2 = 0b0000000000000000010;
        const QUICK_ACTION_3 = 0b0000000000000000100;
        const QUICK_ACTION_4 = 0b0000000000000001000;
        const QUICK_ACTION_5 = 0b0000000000000010000;
        const QUICK_ACTION_6 = 0b0000000000000100000;
        const MODIFIER_1     = 0b0000000000001000000;
        const MODIFIER_2     = 0b0000000000010000000;
        const FACTOR_1       = 0b0000000000100000000;
        const FACTOR_2       = 0b0000000001000000000;
        const PAN_1          = 0b0000000010000000000;
        const PAN_2          = 0b0000000100000000000;
        const WHEEL_X        = 0b0000001000000000000;
        const WHEEL_Y        = 0b0000010000000000000;
        const TEXT_REPLACE   = 0b0000100000000000000;
        /// If a node supports FOCUS_GRAB and the
        /// user sends a QuickAction1, the node
        /// will obtain the focus. It will then
        /// receive DirInput events automatically.
        const FOCUS_GRAB     = 0b0001000000000000000;
        const DIR_INPUT      = 0b0010000000000000000;
        const TEXT_INSERT    = 0b0100000000000000000;
        const TEXT_DELETE    = 0b1000000000000000000;
    }

    /// Which render layer should be cached
    pub struct LayerCaching: u8 {
        const BACKGROUND = 0b01;
        const FOREGROUND = 0b10;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Events supported by this toolkit.
#[derive(Debug, Clone)]
pub enum Event {
    QuickAction1,
    QuickAction2,
    QuickAction3,
    QuickAction4,
    QuickAction5,
    QuickAction6,
    Modifier1(bool),
    Modifier2(bool),
    Factor1(f64),
    Factor2(f64),
    Pan1(usize, usize),
    Pan2(usize, usize),
    WheelX(f64),
    WheelY(f64),
    TextReplace(String),
    FocusGrab(bool),
    /// Nodes which grabbed the focus
    /// receives this special event:
    DirInput(Direction),
    TextInsert(String),
    TextDelete(isize),
}

/// An owned path to a node in a view
pub type NodePath = Vec<usize>;

/// A path to a node in a view
pub type NodePathSlice<'a> = &'a [usize];

/// Trait for elements of a view
pub trait Node: Debug + Any {
    /// `as_any` is required for downcasting.
    fn as_any(&mut self) -> &mut dyn Any;

    /// Implement this if in the following way:
    /// ```rust
    /// fn please_clone(&self) -> NodeBox {
    ///     node_box(self.clone())
    /// }
    /// ```
    fn please_clone(&self) -> NodeBox;

    /// ret = Ok(use_buffer_for_children)
    #[allow(unused)]
    fn render(
        &mut self,
        layer: RenderLayer,
        app: &mut Application,
        path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        match layer {
            RenderLayer::Background => self.render_background(app, path, style, spot, scratch),
            RenderLayer::Foreground => self.render_foreground(app, path, style, spot, scratch),
        }
    }

    /// ret = Ok(use_buffer_for_children)
    #[allow(unused)]
    fn render_background(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        Ok(())
    }

    #[allow(unused)]
    fn render_foreground(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        style: usize,
        spot: &mut Spot,
        scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        Ok(())
    }

    #[allow(unused)]
    fn tick(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        style: usize,
        scratch: ScratchBuffer,
    ) -> Result<bool, ()> {
        Ok(false)
    }

    /// The `handle` method is called when the platform forwards an event
    /// to the application. You can implement this method to receive these
    /// events and maybe react to them.
    ///
    /// Note: You have to report supported events via
    /// [`Node::supported_events`] and [`Node::describe_supported_events`]
    /// to actually receive calls to this method.
    #[allow(unused)]
    fn handle(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        event: &Event,
    ) -> Result<Option<String>, ()> {
        Err(error!("Event {:?} was reportedly supported by `handle` wasn't implemented", event))
    }

    /// Once you add [`DataRequest`](`crate::app::DataRequest`)s to
    /// `app.data_requests`, the platform should fetch the data you
    /// requested. Once it has fetched the data, it will call the
    /// `loaded` method.
    #[allow(unused)]
    fn loaded(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        name: &str,
        offset: usize,
        data: &[u8],
    ) -> Status {
        Err(error!("\"{}\" was loaded but the dst node doesn't implement `loaded`", name))
    }

    /// When instanciating your object implementing [`Node`],
    /// you might find that it is too early for some
    /// work (such as pushing [`DataRequest`](`crate::app::DataRequest`)s).
    /// You can use this method, which is called as soon as
    /// your node is attached to an app's view, to do such
    /// things.
    #[allow(unused)]
    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), ()> {
        Ok(())
    }

    /// General-purpose containers must implement this method
    /// to receive children (for instance while parsing views).
    #[allow(unused)]
    fn add_node(&mut self, child: NodeBox) -> Result<usize, ()> {
        Err(error!("add_node was called but not on a container"))
    }

    /// This method is called when a child of this node is to
    /// be replaced. All General-purpose containers must implement
    /// this.
    #[allow(unused)]
    fn replace_node(&mut self, index: usize, child: NodeBox) -> Result<(), ()> {
        Err(error!("replace_node was called but not on a container"))
    }

    /// This method is called during layout to report overflow
    /// in a container: there is too much content. The value
    /// is a length, in pixels, on the container's axis.
    #[allow(unused)]
    fn set_overflow(&mut self, px_overflow: usize) -> Result<(), ()> {
        Err(error!("set_overflow was called but not on a container"))
    }

    /// This method is mainly called when the toolkit deals
    /// with scrollbars. You should report the value previously
    /// set by [`set_overflow`](Self::set_overflow).
    #[allow(unused)]
    fn get_overflow(&self) -> Result<usize, ()> {
        Err(error!("get_overflow was called but not on a container"))
    }

    /// Nodes can report a margin to the layout algorithm
    /// via this method.
    fn margin(&self) -> Option<Margin> {
        None
    }

    /// The layout code will call this method on every
    /// node to know how it should lay it out. The default
    /// implementation return a fixed length policy of
    /// zero pixels.
    ///
    /// Note: This function must return the same value
    /// for the entire lifetime of the object implementing
    /// [`Node`].
    ///
    /// See [`LengthPolicy`] for a list of policies.
    fn policy(&self) -> LengthPolicy {
        LengthPolicy::Fixed(0)
    }

    /// Used by [`Application`] code to cache
    /// rendered layers efficiently.
    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::empty()
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Err(error!("Node doesn't implement `render_cache`"))
    }

    #[allow(unused)]
    fn store_cache(&mut self, layer: RenderLayer, cache: Vec<u8>) -> Result<(), ()> {
        let index = match layer {
            RenderLayer::Foreground => 0,
            RenderLayer::Background => 1,
        };
        self.render_cache()?[index] = Some(cache);
        Ok(())
    }

    #[allow(unused)]
    fn restore_cache(&mut self, layer: RenderLayer) -> Option<Vec<u8>> {
        let index = match layer {
            RenderLayer::Foreground => 0,
            RenderLayer::Background => 1,
        };
        let mut tmp = None;
        swap(&mut tmp, &mut self.render_cache().ok()?[index]);
        tmp
    }

    /// The `describe` method is called when the platform needs a
    /// textual description of a node. This helps making
    /// applications accessible to people with disabilities.
    fn describe(&self) -> String;

    /// A getter for a node's children. General-purpose
    /// containers must implement this.
    #[allow(unused)]
    fn children(&self) -> &[Option<NodeBox>] {
        &[]
    }

    /// A mutable getter for a node's children. General-purpose
    /// containers must implement this.
    #[allow(unused)]
    fn children_mut(&mut self) -> &mut [Option<NodeBox>] {
        &mut []
    }

    fn style_override(&self) -> Option<usize> {
        None
    }

    /// A getter for a node's spot. The spot size is set
    /// by layout code via [`Node::set_spot_size`].
    fn get_spot_size(&self) -> Size {
        Size::zero()
    }

    /// The layout code may call this method many times
    /// during layout. Renderable Nodes should store the
    /// given spot and give it back when [`Node::get_spot_size`]
    /// is called.
    #[allow(unused)]
    fn set_spot_size(&mut self, size: Size) {
        // do nothing
    }

    /// This is called exactly once after layout so that
    /// nodes can detect spot changes.
    #[allow(unused)]
    fn validate_spot_size(&mut self, prev_size: Size) {
        // do nothing
    }

    /// This is called when the focus changes and this node
    /// is now, or was, in focus. Return true if this node
    /// will make the focus change clearly visible.
    #[allow(unused)]
    fn set_focused(&mut self, focused: bool) -> bool {
        false
    }

    /// General-purpose containers should implement this
    /// method. It allows the layout code to know on which
    /// axis it should lay the children out as well as
    /// the gap to place between each child.
    #[allow(unused)]
    fn container(&self) -> Option<(Axis, usize)> {
        None
    }

    fn cursor(&self, top_left: Point) -> Option<Cursor> {
        let (axis, gap) = self.container()?;
        let row = match self.policy() {
            LengthPolicy::Chunks(row) => Some(row),
            _ => None,
        };
        let size = self.get_spot_size();
        let max_chunk_length = size.get_for_axis(axis);
        Some(Cursor {
            axis,
            gap,
            top_left,
            line_start: top_left,
            row,
            max_chunk_length,
            chunk_length: 0,
        })
    }

    /// The `supported_events` method is called during hit
    /// testing, for platforms with absolute input devices
    /// like mice.
    #[allow(unused)]
    fn supported_events(&self) -> EventType {
        EventType::empty()
    }

    /// The `describe_supported_events` method is called when
    /// the platform needs a textual description of events
    /// supported by a node. This helps making applications
    /// accessible to people with disabilities.
    #[allow(unused)]
    fn describe_supported_events(&self) -> Vec<(EventType, String)> {
        Vec::new()
    }

    fn push_spot_sizes(&self, sizes: &mut Vec<Size>) {
        sizes.push(self.get_spot_size());
        for child in self.children() {
            if let Some(child) = child.as_ref() {
                child.push_spot_sizes(sizes);
            }
        }
    }

    fn detect_size_changes(&mut self, sizes: &mut Vec<Size>) {
        for child in self.children_mut().iter_mut().rev() {
            if let Some(child) = child.as_mut() {
                child.detect_size_changes(sizes);
            }
        }
        if let Some(prev_size) = sizes.pop() {
            let curr_size = self.get_spot_size();
            if curr_size != prev_size {
                self.validate_spot_size(prev_size);
            }
        }
    }

    /// Debug utility
    fn tree_log(&self, app: &Application, tabs: usize) {
        let prefix = "    ".repeat(tabs);
        let size = self.get_spot_size();
        info!("{}<{}> ({}x{})", prefix, self.describe(), size.w, size.h);
        for child in self.children() {
            if let Some(child) = child {
                child.tree_log(app, tabs + 1);
            } else {
                info!("{}    <kidnapped>", prefix);
            }
        }
    }
}

impl RenderReason {
    pub fn downgrade(&mut self) {
        *self = match *self {
            RenderReason::None => RenderReason::None,
            RenderReason::Computation => RenderReason::None,
            RenderReason::Resized => RenderReason::Computation,
        }
    }

    pub fn is_valid(self) -> bool {
        self != RenderReason::None
    }
}

impl RenderLayer {
    pub fn cached(self, cached: LayerCaching) -> bool {
        match self {
            Self::Background => cached.contains(LayerCaching::BACKGROUND),
            Self::Foreground => cached.contains(LayerCaching::FOREGROUND),
        }
    }
}

/// A handle to a [`Node`] implementor.
pub type NodeBox = Box<dyn Node>;

/// This utility function wraps a node
/// implementor in an [`NodeBox`].
pub fn node_box<W: Node>(node: W) -> NodeBox {
    Box::new(node)
}

pub fn please_clone_vec(orig_nodes: &Vec<Option<NodeBox>>) -> Vec<Option<NodeBox>> {
    let mut nodes = Vec::with_capacity(orig_nodes.len());
    for node in orig_nodes {
        nodes.push(node.as_ref().map(|node| node.please_clone()));
    }
    nodes
}

impl Event {
    pub fn event_type(&self) -> EventType {
        match self {
            Event::QuickAction1 => EventType::QUICK_ACTION_1,
            Event::QuickAction2 => EventType::QUICK_ACTION_2,
            Event::QuickAction3 => EventType::QUICK_ACTION_3,
            Event::QuickAction4 => EventType::QUICK_ACTION_4,
            Event::QuickAction5 => EventType::QUICK_ACTION_5,
            Event::QuickAction6 => EventType::QUICK_ACTION_6,
            Event::Modifier1(_) => EventType::MODIFIER_1,
            Event::Modifier2(_) => EventType::MODIFIER_2,
            Event::Factor1(_) => EventType::FACTOR_1,
            Event::Factor2(_) => EventType::FACTOR_2,
            Event::Pan1(_, _) => EventType::PAN_1,
            Event::Pan2(_, _) => EventType::PAN_2,
            Event::WheelX(_) => EventType::WHEEL_X,
            Event::WheelY(_) => EventType::WHEEL_Y,
            Event::TextReplace(_) => EventType::TEXT_REPLACE,
            Event::FocusGrab(_) => EventType::FOCUS_GRAB,
            Event::DirInput(_) => EventType::DIR_INPUT,
            Event::TextInsert(_) => EventType::TEXT_INSERT,
            Event::TextDelete(_) => EventType::TEXT_DELETE,
        }
    }
}

impl Margin {
    pub fn new(top: usize, bottom: usize, left: usize, right: usize) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    pub fn quad(value: usize) -> Self {
        Self {
            top: value,
            bottom: value,
            left: value,
            right: value,
        }
    }

    pub fn total_on(&self, axis: Axis) -> usize {
        match axis {
            Axis::Horizontal => self.left + self.right,
            Axis::Vertical => self.top + self.bottom,
        }
    }
}

impl Axis {
    pub fn is(self, other: Self) -> Option<()> {
        if other == self {
            Some(())
        } else {
            None
        }
    }

    pub fn complement(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}
