use crate::bitmap::RGBA;
use crate::flexbox::compute_tree;
use crate::lock;
use crate::node::rc_node;
use crate::node::Event;
use crate::node::EventType;
use crate::node::NeedsRepaint;
use crate::node::Node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::status;
use crate::BlitKey;
use crate::BlitPath;
use crate::PlatformBlit;
use crate::PlatformLog;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

#[cfg(feature = "text")]
use crate::text::Font;

use core::any::Any;
use core::fmt::Debug;
use core::mem::swap;
use core::ops::Deref;
use core::ops::DerefMut;
use core::ops::Range;

use std::boxed::Box;
use std::string::String;
use std::vec::Vec;

#[cfg(feature = "text")]
use std::collections::HashMap;
#[cfg(feature = "text")]
use std::sync::Arc;
#[cfg(feature = "text")]
use std::sync::Mutex;

/// Event Handlers added to the app via
/// [`Application::add_handler`] must
/// have this signature.
pub type EventHandler = Box<dyn FnMut(&mut Application, &NodePath, &Event) -> Status>;

/// The Application structure represents your application.
///
/// It stores the currently displayed view, your model and
/// some platform functions
pub struct Application {
    /// This is the root node of the currently displayed view.
    pub view: RcNode,

    /// The spot where our view should be displayed on the
    /// output. It is set by [`Application::set_spot`].
    pub view_spot: Spot,

    /// Fonts that can be used by nodes to draw glyphs
    #[cfg(feature = "text")]
    pub fonts: HashMap<Option<String>, Arc<Mutex<Font>>>,

    /// Some nodes support custom event handlers; when
    /// they need to call the handler, they will use this
    /// field.
    pub event_handlers: HashMap<String, EventHandler>,

    /// Default font size used by textual nodes
    #[cfg(feature = "text")]
    pub default_font_size: usize,

    /// Data requests allow nodes to load external assets,
    /// partially or completely. If you're implementing
    /// [`Node`], pushes to this field are expected in
    /// [`Node::initialize`] and in [`Node::render`].
    pub data_requests: Vec<DataRequest>,

    /// This field's content is completely up to you. You
    /// should use it to store the global state of your
    /// application. Note that you can downcast this to
    /// your structure using [`Application::model`].
    pub model: Box<dyn Any>,

    /// This field has a path which points to the node
    /// which currently has user focus.
    pub focus: Option<(Point, NodePath)>,

    /// A platform-specific function which allows logging
    /// messages. Do not use it directly, prefer the
    /// [`Application::log`] method.
    pub platform_log: PlatformLog,

    /// A platform-specific function which is used to request
    /// buffers where nodes are rendered. Do not use it
    /// directly, prefer the [`Application::blit`] method.
    pub platform_blit: PlatformBlit,

    /// Nodes can prevent children (direct or indirect) from
    /// requesting new buffers to be rendered in by pushing
    /// to this vector; When these children would request a
    /// buffer, that node's buffer will be returned instead.
    /// [`Paragraph`](`crate::text::Paragraph`) nodes use this, for instance,
    /// so that its contained [`GlyphNode`](`crate::text::GlyphNode`) render
    /// in the paragraph's buffer.
    pub blit_hooks: Vec<(NodePath, Spot)>,

    /// The platform will push styles to this vector. All
    /// platforms must push the same number of styles,
    /// however this number is yet to be decided.
    /// You can use this vector in your implementation of
    /// [`Node::render`], using the `style` parameter of that
    /// method.
    pub styles: Vec<Style>,

    /// Global override for node's repaint flags
    pub global_repaint: NeedsRepaint,

    /// Setting this to `true` will trigger a new computation
    /// of the layout at the beginning of the next frame's
    /// rendering.
    pub should_recompute: bool,

    /// Applications using this toolkit can enable visual
    /// debugging of containers by setting this to true.
    pub debug_containers: bool,

    /// Number of milliseconds since instanciation
    pub instance_age_ms: usize,
}

/// Data requests allow nodes to load external assets,
/// partially or completely.
///
/// You can push new ones
/// to `app.data_requests`.
#[derive(Debug, Clone, Hash)]
pub struct DataRequest {
    /// The path to the node which is making the request.
    pub node: NodePath,
    /// the name of the asset (eg. `"img/image0.png"`)
    pub name: String,
    /// If specified, the range of bytes to load.
    pub range: Option<Range<usize>>,
}

/// A color represented as four bytes.
pub type Color = [u8; RGBA];

/// Represent a node's visual style.
#[derive(Debug, Copy, Clone)]
pub struct Style {
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
}

impl Application {
    /// The Application constructor. You should pass the `log` and `blit`
    /// implementations of your platform. To use an XML file as view,
    /// use [`ViewLoader`](`crate::xml::ViewLoader`).
    ///
    /// ```rust
    /// use platform::app;
    /// use platform::log;
    /// use platform::blit;
    /// use acrylic::app::Application;
    /// use acrylic::xml::ViewLoader;
    ///
    /// app!("./", {
    ///     let loader = ViewLoader::new("default.xml");
    ///     let mut app = Application::new(&log, &blit, (), loader);
    ///     app
    /// });
    /// ```
    pub fn new<M: Any + 'static>(
        log: PlatformLog,
        blit: PlatformBlit,
        model: M,
        view: impl Node,
    ) -> Self {
        #[allow(unused_mut)]
        let mut app = Self {
            view: rc_node(view),
            view_spot: (Point::zero(), Size::zero()),
            event_handlers: HashMap::new(),
            #[cfg(feature = "text")]
            fonts: HashMap::new(),
            #[cfg(feature = "text")]
            default_font_size: 30,
            data_requests: Vec::new(),
            model: Box::new(model),
            should_recompute: true,
            global_repaint: NeedsRepaint::empty(),
            debug_containers: false,
            styles: Vec::new(),
            platform_log: log,
            platform_blit: blit,
            blit_hooks: Vec::new(),
            focus: None,
            instance_age_ms: 0,
        };
        app.initialize_node(app.view.clone(), &mut NodePath::new())
            .unwrap();
        #[cfg(all(feature = "text", feature = "noto-default-font"))]
        {
            let font = Font::from_bytes(include_bytes!("noto-sans-regular.ttf").to_vec());
            app.fonts.insert(None, font);
        }
        app
    }

    /// This getter allows you to get your model as its initial
    /// type. If `M` is the original type of your model, this
    /// will return Some, and None if it is not.
    ///
    /// Under the hood, this is a simple downcast.
    pub fn model<M: Any + 'static>(&mut self) -> Option<&mut M> {
        self.model.downcast_mut::<M>()
    }

    /// Adds a font to the font store. If `default` is `true`,
    /// this font will be used by default when textual nodes
    /// are created without a specific font.
    #[cfg(feature = "text")]
    pub fn add_font(&mut self, name: String, data: Vec<u8>, default: bool) {
        let font = Font::from_bytes(data);
        self.fonts.insert(Some(name), font.clone());
        if default {
            self.fonts.insert(None, font);
        }
    }

    /// Platforms should update the instance's age via this
    /// function. This age must only go bigger and bigger.
    pub fn set_age(&mut self, milliseconds: usize) {
        self.instance_age_ms = milliseconds;
    }

    /// Adds an event handler to the application.
    /// Once added, it can be called by nodes supporting
    /// custom event handlers.
    pub fn add_handler(&mut self, name: String, handler: EventHandler) {
        self.event_handlers.insert(name, handler);
    }

    /// Platforms which support pointing input devices (mice)
    /// must use this function to report device movement.
    pub fn pointing_at(&mut self, point: Point) {
        let mut focus = Some((point, self.hit_test(point)));
        swap(&mut self.focus, &mut focus);
        if focus != self.focus {
            if let Some((_, mut path)) = focus {
                loop {
                    if let Some(rc_node) = self.get_node(&path) {
                        let visible_change = { lock(&rc_node).unwrap().set_focused(false) };
                        if visible_change {
                            let _ = self.repaint_needed(rc_node, NeedsRepaint::all());
                        }
                    }
                    if let None = path.pop() {
                        break;
                    }
                }
            }
            if let Some((_, mut path)) = self.focus.clone() {
                loop {
                    if let Some(rc_node) = self.get_node(&path) {
                        let visible_change = { lock(&rc_node).unwrap().set_focused(true) };
                        if visible_change {
                            let _ = self.repaint_needed(rc_node, NeedsRepaint::all());
                        }
                    }
                    if let None = path.pop() {
                        break;
                    }
                }
            }
        }
    }

    /// Platforms can detect focus-grabbing nodes via
    /// this method.
    pub fn can_grab_focus(&self, except: Option<EventType>) -> bool {
        let mut result = false;
        if let Some((_, mut path)) = self.focus.clone() {
            loop {
                if let Some(node) = self.get_node(&path) {
                    let node = lock(&node).unwrap();
                    let event_mask = node.supported_events();
                    if event_mask.contains(EventType::FOCUS_GRAB) {
                        result = true;
                        break;
                    }
                    if let Some(except) = except {
                        if event_mask.contains(except) {
                            break;
                        }
                    }
                }
                if let None = path.pop() {
                    break;
                }
            };
        }
        result
    }

    /// Platforms should trigger input events via
    /// this method.
    pub fn fire_event(&mut self, event: &Event) -> Status {
        let mut result = Err(());
        if let Some((_, mut path)) = self.focus.clone() {
            let handler_name = loop {
                if let Some(node) = self.get_node(&path) {
                    let mut node = lock(&node).unwrap();
                    let event_mask = node.supported_events();
                    if event_mask.contains(event.event_type()) {
                        break node.handle(self, &path, event)?;
                    }
                }
                if let None = path.pop() {
                    break None;
                }
            };
            if let Some(name) = handler_name {
                let handler = self.event_handlers.remove(&name);
                if let Some(mut handler) = handler {
                    result = (handler)(self, &path, event);
                    self.event_handlers.insert(name, handler);
                }
            }
        }
        result
    }

    /// Platforms can ask what events the application will accept
    /// via this function. It can be called after any input event.
    pub fn acceptable_events(&mut self) -> Vec<(EventType, String)> {
        let mut events = Vec::new();
        let mut pushed = EventType::empty();
        if let Some((_, mut path)) = self.focus.clone() {
            loop {
                if let Some(node) = self.get_node(&path) {
                    let node = lock(&node).unwrap();
                    for (event, description) in node.describe_supported_events() {
                        if !pushed.contains(event) {
                            events.push((event, description));
                            pushed.insert(event);
                        }
                    }
                }
                if let None = path.pop() {
                    break;
                }
            }
        }
        events
    }

    pub fn hit_test(&mut self, point: Point) -> NodePath {
        let mut path = NodePath::new();
        if !Self::hit_test_for(self.view.clone(), point, &mut path) {
            path.clear()
        }
        path
    }

    fn hit_test_for(node: RcNode, p: Point, path: &mut NodePath) -> bool {
        let ((min, size), children) = {
            let node = lock(&node).unwrap();
            (node.get_spot(), node.children().to_vec())
        };
        let max_x = min.x + size.w as isize;
        let max_y = min.y + size.h as isize;
        if (min.x..max_x).contains(&p.x) && (min.y..max_y).contains(&p.y) {
            for i in 0..children.len() {
                path.push(i);
                if Self::hit_test_for(children[i].clone(), p, path) {
                    return true;
                }
                path.pop();
            }
            return true;
        }
        return false;
    }

    /// The platforms should use this method to add styles
    /// as soon as they have a handle to an [`Application`].
    /// They can call it again between calls to
    /// [`Application::render`] to change styles.
    pub fn set_styles(&mut self, styles: Vec<Style>) {
        self.styles = styles;
        self.global_repaint = NeedsRepaint::all();
    }

    /// Use this method to find a node in the view based
    /// on its path.
    pub fn get_node(&self, path: &NodePath) -> Option<RcNode> {
        let mut node = self.view.clone();
        for i in path.as_slice() {
            // todo: get rid of these locks
            let child = {
                let tmp = lock(&node)?;
                tmp.children().get(*i)?.clone()
            };
            node = child;
        }
        Some(node)
    }

    pub fn replace_node(&mut self, path: &NodePath, new_node: RcNode) -> Result<(), String> {
        self.should_recompute = true;
        if let Some(j) = path.last() {
            let mut node = self.view.clone();
            for i in &path[..path.len() - 1] {
                // todo: get rid of these locks
                let child = {
                    let tmp = lock(&node).unwrap();
                    tmp.children()[*i].clone()
                };
                node = child;
            }
            let mut tmp = lock(&node).unwrap();
            tmp.replace_node(*j, new_node.clone())?;
        } else {
            self.view = new_node.clone();
            let mut view = lock(&self.view).unwrap();
            view.set_spot(self.view_spot);
        }
        let mut path = path.clone();
        self.initialize_node(new_node, &mut path)
    }

    /// Signals that part of the view needs to be repainted.
    ///
    /// The node at path and its children (direct and
    /// indirect) will be affected.
    pub fn repaint_needed(&mut self, node: RcNode, mut r: NeedsRepaint) -> Status {
        let mut path = Vec::new();
        for_each_node(node, &mut path, &mut r, (), |n, r, _, _| {
            n.repaint_needed(*r)
        })
    }

    /// Platforms should use this method to set the position
    /// and size of the view in the output buffer.
    ///
    /// Returns `true` if the previous and new values differ.
    pub fn set_spot(&mut self, spot: Spot) -> bool {
        if self.view_spot != spot {
            self.view_spot = spot;
            let mut view = lock(&self.view).unwrap();
            let view = view.deref_mut();
            view.set_spot(spot);
            self.should_recompute = true;
            self.global_repaint = NeedsRepaint::BACKGROUND | NeedsRepaint::OVERLAY;
            true
        } else {
            false
        }
    }

    /// This method is called by the platform to request a refresh
    /// of the output. It should be called for every frame.
    pub fn render(&mut self) {
        let mut path = Vec::new();
        let max_runs = 5;
        let mut runs = 0;
        loop {
            if self.should_recompute {
                self.log("recomputing layout");
                {
                    let view = lock(&self.view).unwrap();
                    let _ = compute_tree(view.deref());
                }
                path.clear();
                let _ = for_each_node(self.view.clone(), &mut path, &mut (), (), |n, _, _, _| {
                    n.validate_spot()
                });
                for i in 0..self.blit_hooks.len() {
                    if let Some(node) = self.get_node(&self.blit_hooks[i].0) {
                        let node = lock(&node).unwrap();
                        let spot = node.get_content_spot();
                        let spot = spot.unwrap_or((Point::zero(), Size::zero()));
                        self.blit_hooks[i].1 = spot;
                    }
                }
                self.should_recompute = false;
            }
            if !self.global_repaint.is_empty() {
                if self.global_repaint.contains(NeedsRepaint::OVERLAY) {
                    let spot = self.view_spot;
                    let (dst, pitch, _) = self.blit(&spot, BlitPath::Overlay).unwrap();
                    for_each_line(dst, spot.1, pitch, |_, line_dst| line_dst.fill(0));
                }
                let _ = self.repaint_needed(self.view.clone(), self.global_repaint);
                self.global_repaint = NeedsRepaint::empty();
            }
            path.clear();
            let _ = for_each_node(self.view.clone(), &mut path, self, 0, |n, a, s, p| {
                n.render(a, p, s).unwrap_or(s)
            });
            if (!self.should_recompute && self.global_repaint.is_empty()) || max_runs == runs {
                if max_runs == runs {
                    self.log("warning: runs == max_runs in app::render()");
                }
                break;
            } else {
                runs += 1;
            }
        }
    }

    fn initialize_node(&mut self, node: RcNode, path: &mut NodePath) -> Result<(), String> {
        let children = {
            let mut node = lock(&node).unwrap();
            node.initialize(self, path)?;
            node.children().to_vec()
        };
        for i in 0..children.len() {
            path.push(i);
            self.initialize_node(children[i].clone(), path)?;
            path.pop();
        }
        Ok(())
    }

    /// Anyone can use this method to log messages.
    pub fn log(&self, message: &str) {
        (self.platform_log)(message)
    }

    /// Nodes can use this method to request a buffer where
    /// they can then be rendered.
    ///
    /// On success, the return value is a tuple containing:
    /// 1. the buffer slice
    /// 2. the pitch (number of bytes that you should skip between lines)
    /// 3. false if this buffer is shared between nodes at this spot, true otherwise
    ///
    /// You should use these values with [`for_each_line`]
    /// in your rendering code instead of using them directly,
    /// as it is easy to trigger panics when doing so.
    pub fn blit<'a>(
        &'a self,
        node_spot: &'a Spot,
        path: BlitPath<'a>,
    ) -> Result<(&'a mut [u8], usize, bool), ()> {
        if let BlitPath::Node(path) = &path {
            for (hook_path, hook_spot) in &self.blit_hooks {
                if path.starts_with(hook_path) {
                    let key = BlitPath::Node(hook_path).to_key();
                    let (slice, pitch, owned) = status((self.platform_blit)(*hook_spot, key))?;
                    let (slice, pitch) = status(sub_spot(slice, pitch, [*hook_spot, *node_spot]))?;
                    return Ok((slice, pitch, owned));
                }
            }
        }
        let key = path.to_key();
        let spot = match key {
            BlitKey::Background => self.view_spot,
            BlitKey::Overlay => self.view_spot,
            _ => *node_spot,
        };
        let (mut slice, mut pitch, owned) = status((self.platform_blit)(spot, key))?;
        if let BlitKey::Background | BlitKey::Overlay = key {
            (slice, pitch) = status(sub_spot(slice, pitch, [spot, *node_spot]))?;
        }
        Ok((slice, pitch, owned))
    }
}

pub fn for_each_node<T, U: Copy>(
    node: RcNode,
    path: &mut NodePath,
    arg1: &mut T,
    arg2: U,
    f: impl Copy + Fn(&mut dyn Node, &mut T, U, &mut NodePath) -> U,
) -> Status {
    let (children, arg2) = {
        let mut node = status(lock(&node))?;
        let arg2 = f(node.deref_mut(), arg1, arg2, path);
        (node.children().to_vec(), arg2)
    };
    for i in 0..children.len() {
        path.push(i);
        for_each_node(children[i].clone(), path, arg1, arg2, f)?;
        path.pop();
    }
    Ok(())
}

/// This utility function tries to crop a buffer
/// into a smaller view of this buffer.
///
/// `spots` should contain the original spot of the
/// buffer and the smaller spot, in that order respectively.
pub fn sub_spot<'a>(
    slice: &'a mut [u8],
    mut pitch: usize,
    spots: [Spot; 2],
) -> Option<(&'a mut [u8], usize)> {
    let [(hp, hs), (np, ns)] = spots;
    if ns.w != 0 && ns.h != 0 {
        if ns.w <= hs.w && ns.h <= hs.h {
            let x: usize = (np.x - hp.x).try_into().ok()?;
            let y: usize = (np.y - hp.y).try_into().ok()?;
            pitch += RGBA * (hs.w - ns.w);
            let line = pitch + RGBA * ns.w;
            let start = RGBA * x + y * line;
            let stop = start + ns.h * line - pitch;
            Some((slice.get_mut(start..stop)?, pitch))
        } else {
            None
        }
    } else {
        Some((&mut [], 0))
    }
}

/// This utility function calls `f` for each line
/// in a buffer.
///
/// These line will have a length of `size.w` and
/// there will be `size.h` calls. The first argument
/// of `f` is the line number, starting from `0`.
pub fn for_each_line(
    slice: &mut [u8],
    size: Size,
    pitch: usize,
    mut f: impl FnMut(usize, &mut [u8]),
) {
    let px_width = size.w * RGBA;
    let mut start = 0;
    let mut stop = px_width;
    let advance = px_width + pitch;
    for i in 0..size.h {
        f(i, &mut slice[start..stop]);
        start += advance;
        stop += advance;
    }
}
