use bitflags::bitflags;

use crate::app::for_each_line;
use crate::app::Application;
use crate::bitmap::RGBA;
use crate::BlitPath;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

#[cfg(feature = "railway")]
use crate::railway::arg;
#[cfg(feature = "railway")]
use crate::railway::LoadedRailwayProgram;
#[cfg(feature = "railway")]
use lazy_static::lazy_static;
#[cfg(feature = "railway")]
use railway::Couple;
#[cfg(feature = "railway")]
use railway::Program;

use core::any::Any;
use core::fmt::Debug;

use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

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

/// This can be used by [`Node`] implementations
/// to offset the boundaries of their original
/// rendering spot.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Margin {
    pub top: isize,
    pub bottom: isize,
    pub left: isize,
    pub right: isize,
}

bitflags! {
    /// This is a bit field of events supported
    /// by this toolkit.
    pub struct EventType: u32 {
        const QUICK_ACTION_1 = 0b0000000000000001;
        const QUICK_ACTION_2 = 0b0000000000000010;
        const QUICK_ACTION_3 = 0b0000000000000100;
        const QUICK_ACTION_4 = 0b0000000000001000;
        const QUICK_ACTION_5 = 0b0000000000010000;
        const QUICK_ACTION_6 = 0b0000000000100000;
        const MODIFIER_1     = 0b0000000001000000;
        const MODIFIER_2     = 0b0000000010000000;
        const FACTOR_1       = 0b0000000100000000;
        const FACTOR_2       = 0b0000001000000000;
        const PAN_1          = 0b0000010000000000;
        const PAN_2          = 0b0000100000000000;
        const WHEEL_X        = 0b0001000000000000;
        const WHEEL_Y        = 0b0010000000000000;
        const TEXT_INPUT     = 0b0100000000000000;
    }

    /// Utility bit field to keep track of what you
    /// need to repaint
    pub struct NeedsRepaint: u8 {
        const BACKGROUND = 0b01;
        const FOREGROUND = 0b10;
    }
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
    TextInput(String),
}

/// A path to a node in a view
pub type NodePath = Vec<usize>;

pub type NodePathHash = u64;

/// Trait for elements of a view
pub trait Node: Debug + Any + 'static {
    /// `as_any` is required for downcasting.
    fn as_any(&mut self) -> &mut dyn Any;

    #[allow(unused)]
    /// This method is called each time a new frame
    /// is being created for display.
    ///
    /// The `style` parameter is an index into `app.styles`.
    /// Nodes should follow this style when applicable.
    /// For containers, the returned `usize` is the style
    /// index which will be passed to its children. For
    /// non-containers, this `usize` has no effect.
    ///
    /// Example implementation filling the spot with white:
    /// ```rust
    /// use acrylic::Spot;
    /// use acrylic::node::Node;
    /// use acrylic::node::NodePath;
    /// use acrylic::node::NeedsRepaint;
    /// use acrylic::node::LengthPolicy;
    /// use acrylic::app::Application;
    /// use acrylic::app::for_each_line;
    /// use core::any::Any;
    ///
    /// #[derive(Debug, Copy, Clone)]
    /// struct MyNode {
    ///     repaint: NeedsRepaint,
    ///     spot: Spot,
    /// }
    ///
    /// impl Node for MyNode {
    ///     fn render(&mut self, app: &mut Application, path: &mut NodePath, _: usize) -> Result<usize, ()> {
    ///         if self.repaint.contains(NeedsRepaint::FOREGROUND) {
    ///             if let Ok((dst, pitch, _)) = app.blit(&self.spot, Some(path)) {
    ///                 let (_, size) = self.spot;
    ///                 for_each_line(dst, size, pitch, |_, line| {
    ///                     line.fill(255);
    ///                 });
    ///             } else {
    ///                 app.log("rendering failed.");
    ///             }
    ///             self.repaint.remove(NeedsRepaint::FOREGROUND);
    ///         }
    ///         Ok(0)
    ///     }
    ///
    ///     // other methods necessary for rendering:
    ///
    ///     fn as_any(&mut self) -> &mut dyn Any {
    ///         self
    ///     }
    ///
    ///     fn describe(&self) -> String {
    ///         String::from("White Square")
    ///     }
    ///
    ///     fn repaint_needed(&mut self, repaint: NeedsRepaint) {
    ///         self.repaint.insert(repaint);
    ///     }
    ///
    ///     fn policy(&self) -> LengthPolicy {
    ///         LengthPolicy::AspectRatio(1.0)
    ///     }
    ///
    ///     fn get_spot(&self) -> Spot {
    ///         self.spot
    ///     }
    ///
    ///     fn set_spot(&mut self, spot: Spot) {
    ///         self.spot = spot;
    ///         // we could need a repaint:
    ///         self.repaint.insert(NeedsRepaint::FOREGROUND);
    ///     }
    /// }
    /// ```
    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        style: usize,
    ) -> Result<usize, ()> {
        Err(())
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
        path: &NodePath,
        event: &Event,
    ) -> Result<Option<String>, ()> {
        Err(())
    }

    /// Once you add [`DataRequest`](`crate::app::DataRequest`)s to
    /// `app.data_requests`, the platform should fetch the data you
    /// requested. Once it has fetched the data, it will call the
    /// `loaded` method.
    #[allow(unused)]
    fn loaded(
        &mut self,
        app: &mut Application,
        path: &NodePath,
        name: &str,
        offset: usize,
        data: &[u8],
    ) -> Status {
        Err(())
    }

    /// When instanciating your object implementing [`Node`],
    /// you might find that it is too early for some
    /// work (such as pushing [`DataRequest`](`crate::app::DataRequest`)s).
    /// You can use this method, which is called as soon as
    /// your node is attached to an app's view, to do such
    /// things.
    #[allow(unused)]
    fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
        Ok(())
    }

    /// General-purpose containers must implement this method
    /// to receive children (for instance while parsing views).
    #[allow(unused)]
    fn add_node(&mut self, child: RcNode) -> Result<usize, String> {
        Err(String::from("Not a container"))
    }

    /// This method is called when a child of this node is to
    /// be replaced. All General-purpose containers must implement
    /// this.
    #[allow(unused)]
    fn replace_node(&mut self, index: usize, child: RcNode) -> Result<(), String> {
        Err(String::from("Not a container"))
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

    /// Used by [`Application`] code to force a node
    /// to repaint during next frame rendering.
    #[allow(unused)]
    fn repaint_needed(&mut self, repaint: NeedsRepaint) {
        // do nothing by default
    }

    /// The `describe` method is called when the platform needs a
    /// textual description of a node. This helps making
    /// applications accessible to people with disabilities.
    fn describe(&self) -> String;

    /// A getter for a node's children. General-purpose
    /// containers must implement this.
    #[allow(unused)]
    fn children(&self) -> &[RcNode] {
        &[]
    }

    /// A getter for a node's spot. The spot
    /// is set by layout code via [`Node::set_spot`].
    fn get_spot(&self) -> Spot {
        (Point::zero(), Size::zero())
    }

    /// Offsets a spot by a node's margin. It should
    /// never be required to implement this.
    fn get_content_spot_at(&self, mut spot: Spot) -> Option<Spot> {
        if let Some(margin) = self.margin() {
            spot.0.x += margin.left;
            spot.0.y += margin.top;
            let w = ((spot.1.w as isize) - margin.total_on(Axis::Horizontal)).try_into();
            let h = ((spot.1.h as isize) - margin.total_on(Axis::Vertical)).try_into();
            match (w, h) {
                (Ok(w), Ok(h)) => spot.1 = Size::new(w, h),
                _ => None?,
            }
        }
        Some(spot)
    }

    /// Offsets a node's spot by that node's margin.
    /// It should never be required to implement this.
    fn get_content_spot(&self) -> Option<Spot> {
        self.get_content_spot_at(self.get_spot())
    }

    /// The layout code may call this method many times
    /// during layout. Renderable Nodes should store the
    /// given spot and give it back when [`Node::get_spot`]
    /// is called.
    #[allow(unused)]
    fn set_spot(&mut self, spot: Spot) {
        // do nothing
    }

    /// This is called exactly once after layout so that
    /// nodes can detect spot changes.
    fn validate_spot(&mut self) {
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
}

/// A handle to a [`Node`] implementor.
pub type RcNode = Arc<Mutex<dyn Node>>;

/// This utility function wraps a node
/// implementor in an [`RcNode`].
pub fn rc_node<W: Node>(node: W) -> RcNode {
    Arc::new(Mutex::new(node))
}

#[cfg(feature = "railway")]
lazy_static! {
    static ref CONTAINER_RWY: LoadedRailwayProgram<4> = {
        let program = Program::parse(include_bytes!("container.rwy")).unwrap();
        let mut stack = program.create_stack();
        program.valid().unwrap();
        let mut addresses = [0; 4];
        {
            let arg = |s| arg(&program, s, true).unwrap();
            addresses[0] = arg("size");
            addresses[1] = arg("margin-radius");
            addresses[2] = arg("background-color-red-green");
            addresses[3] = arg("background-color-blue-alpha");
            stack[arg("border-width")].x = 0.0;
            stack[arg("border-pattern")].x = 0.0;
            stack[arg("border-pattern")].y = 10.0;
            stack[arg("border-color-blue-alpha")].y = 0.0;
        }
        LoadedRailwayProgram {
            program,
            stack,
            mask: Vec::new(),
            addresses,
        }
    };
}

/// General-purpose container
#[derive(Debug, Clone)]
pub struct Container {
    pub children: Vec<RcNode>,
    pub policy: LengthPolicy,
    pub on_click: Option<String>,
    pub spot: Spot,
    pub prev_spot: Spot,
    pub axis: Axis,
    pub gap: usize,
    pub margin: Option<usize>,
    /// For rounded-corners
    pub radius: Option<usize>,
    /// Initialize to `NeedsRepaint::all()`
    pub repaint: NeedsRepaint,
    pub focused: bool,
    /// Style override
    pub normal_style: Option<usize>,
    /// Style override when focused 
    pub focus_style: Option<usize>,
    /// Initialize to `None`
    #[cfg(feature = "railway")]
    pub style_rwy: Option<LoadedRailwayProgram<4>>,
}

impl Container {
    fn style(&self) -> Option<usize> {
        match self.focused {
            true => self.focus_style.or(self.normal_style),
            false => self.normal_style,
        }
    }
}

impl Node for Container {
    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        style: usize,
    ) -> Result<usize, ()> {
        let (_, size) = self.spot;
        let px_width = RGBA * size.w;
        if self.repaint.contains(NeedsRepaint::FOREGROUND) {
            self.repaint.remove(NeedsRepaint::FOREGROUND);
            #[cfg(feature = "railway")]
            if self.margin.is_some() || self.radius.is_some() {
                if self.style_rwy.is_none() {
                    self.style_rwy = Some(CONTAINER_RWY.clone());
                }
                if let Some(rwy) = &mut self.style_rwy {
                    let parent_bg = app.styles[style].background;
                    let c = |i| parent_bg[i] as f32 / 255.0;
                    let margin = self.margin.unwrap_or(1);
                    let radius = self.radius.unwrap_or(1);
                    // size
                    rwy.stack[rwy.addresses[0]] = Couple::new(size.w as f32, size.h as f32);
                    // margin and radius
                    rwy.stack[rwy.addresses[1]] = Couple::new(margin as f32, radius as f32);
                    // parent RG and BA
                    rwy.stack[rwy.addresses[2]] = Couple::new(c(0), c(1));
                    rwy.stack[rwy.addresses[3]] = Couple::new(c(2), c(3));
                    let (dst, pitch, _) = app.blit(&self.spot, BlitPath::Node(path))?;
                    rwy.render(dst, pitch, size)?;
                }
            }
            if app.debug_containers {
                let (dst, pitch, _) = app.blit(&self.spot, BlitPath::Node(path))?;
                for_each_line(dst, size, pitch, |i, line_dst| {
                    if i == 0 {
                        line_dst.fill(255);
                    } else {
                        line_dst[RGBA..].fill(0);
                        line_dst[..RGBA].fill(255);
                    }
                });
            }
        }
        if self.repaint.contains(NeedsRepaint::BACKGROUND) {
            self.repaint.remove(NeedsRepaint::BACKGROUND);
            if let Some(i) = self.style() {
                let this_bg = app.styles[i].background;
                let (dst, pitch, _) = app.blit(&self.spot, BlitPath::Background)?;
                for_each_line(dst, size, pitch, |_, line_dst| {
                    for i in 0..px_width {
                        line_dst[i] = this_bg[i % RGBA];
                    }
                });
            }
        }
        Ok(self.style().unwrap_or(style))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn margin(&self) -> Option<Margin> {
        self.margin.map(|l| Margin::quad(l as isize))
    }

    fn children(&self) -> &[RcNode] {
        &self.children
    }

    fn policy(&self) -> LengthPolicy {
        self.policy
    }

    fn add_node(&mut self, child: RcNode) -> Result<usize, String> {
        let index = self.children.len();
        self.children.push(child);
        Ok(index)
    }

    fn replace_node(&mut self, index: usize, child: RcNode) -> Result<(), String> {
        match self.children.get_mut(index) {
            Some(addr) => *addr = child,
            None => Err(String::from("No such child :|"))?,
        };
        Ok(())
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
    }

    fn validate_spot(&mut self) {
        if self.spot != self.prev_spot {
            self.repaint = NeedsRepaint::all();
        }
        self.prev_spot = self.spot;
    }

    fn repaint_needed(&mut self, repaint: NeedsRepaint) {
        self.repaint.insert(repaint);
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        self.focused = focused;
        self.focus_style.is_some()
    }

    fn container(&self) -> Option<(Axis, usize)> {
        Some((self.axis, self.gap))
    }

    fn describe(&self) -> String {
        String::from(match self.axis {
            Axis::Vertical => "Vertical Container",
            Axis::Horizontal => "Horizontal Container",
        })
    }

    fn handle(
        &mut self,
        _: &mut Application,
        _: &NodePath,
        _: &Event,
    ) -> Result<Option<String>, ()> {
        Ok(self.on_click.clone())
    }

    fn supported_events(&self) -> EventType {
        EventType::QUICK_ACTION_1
    }

    fn describe_supported_events(&self) -> Vec<(EventType, String)> {
        let mut events = Vec::new();
        if self.on_click.is_some() {
            events.push((EventType::QUICK_ACTION_1, String::from("Some action")));
        }
        events
    }
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
            Event::TextInput(_) => EventType::TEXT_INPUT,
        }
    }
}

impl Margin {
    pub fn new(top: isize, bottom: isize, left: isize, right: isize) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    pub fn quad(value: isize) -> Self {
        Self {
            top: value,
            bottom: value,
            left: value,
            right: value,
        }
    }

    pub fn total_on(&self, axis: Axis) -> isize {
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

pub(crate) trait SameAxisContainerOrNone {
    fn same_axis_or_both_none(self) -> bool;
}

impl SameAxisContainerOrNone for (Option<(Axis, usize)>, Option<(Axis, usize)>) {
    fn same_axis_or_both_none(self) -> bool {
        match self {
            (Some((a, _)), Some((b, _))) => a == b,
            (None, None) => true,
            _ => false,
        }
    }
}
