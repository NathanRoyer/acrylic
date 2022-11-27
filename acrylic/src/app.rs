//! Application, DataRequest, EventHandler, ScratchBuffer

use crate::flexbox::compute_tree;
use crate::style::Theme;
use crate::node::node_box;
use crate::node::Event;
use crate::node::EventType;
use crate::node::RenderLayer;
use crate::node::Node;
use crate::node::NodePath;
use crate::node::NodePathSlice;
use crate::node::NodeBox;
use crate::bitmap::RGBA;
use crate::status;
use crate::PlatformLog;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;
use crate::format;

use core::any::Any;
use core::fmt::Debug;
use core::mem::swap;
use core::ops::Range;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use hashbrown::hash_map::HashMap;

#[cfg(feature = "text")]
use crate::font::GlyphCache;

/// Event Handlers added to the app via
/// [`Application::add_handler`] must
/// have this signature.
pub type EventHandler = Box<dyn FnMut(&mut Application, NodePathSlice, &Event) -> Status>;

/// A scratch buffer is required for certain rendering
/// operations.
pub type ScratchBuffer<'a> = &'a mut Vec<u8>;

/// The Application structure represents your application.
///
/// It stores the currently displayed view, your model and
/// some platform functions
pub struct Application {
    /// This is the root node of the currently displayed view.
    pub view: Option<NodeBox>,

    /// The spot where our view should be displayed on the
    /// output. It is set by [`Application::set_fb_size`].
    pub fb_size: Size,

    /// Hashmap to translate font names into their index
    /// in `self.fonts`.
    #[cfg(feature = "text")]
    pub font_ns: HashMap<String, usize>,

    /// Fonts that can be used by nodes to draw glyphs.
    ///
    /// The byte array is parsed each time we need to create
    /// renderings for glyphs, which is not often due to
    /// caching of these renders. Additionally, `ttf-parser`,
    /// which is the crate we use to parse the fonts, says
    /// it shouldn't be a bottleneck. `¯\_(ツ)_/¯`
    ///
    /// If you enabled the `noto-default-font` feature,
    /// the font will be present at index 0.
    #[cfg(feature = "text")]
    pub fonts: Vec<Vec<u8>>,

    /// A cache of rendered glyphs
    #[cfg(feature = "text")]
    pub glyph_cache: GlyphCache,

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

    /// The platform can set the theme to use via the
    /// [`Application::set_theme`] method.
    pub theme: Theme,

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
        model: M,
        view: impl Node,
    ) -> Self {
        #[allow(unused_mut)]
        let mut app = Self {
            view: Some(node_box(view)),
            fb_size: Size::zero(),
            event_handlers: HashMap::new(),
            #[cfg(feature = "text")]
            font_ns: HashMap::new(),
            #[cfg(feature = "text")]
            fonts: Vec::new(),
            #[cfg(feature = "text")]
            glyph_cache: GlyphCache::new(),
            #[cfg(feature = "text")]
            default_font_size: 30,
            data_requests: Vec::new(),
            model: Box::new(model),
            should_recompute: true,
            debug_containers: false,
            theme: Theme::parse(include_str!("default-theme.json")).unwrap(),
            platform_log: log,
            focus: None,
            instance_age_ms: 0,
        };
        app.initialize_node(&mut NodePath::new())
            .unwrap();
        #[cfg(all(feature = "text", feature = "noto-default-font"))]
        {
            app.fonts.push(include_bytes!("noto-sans.ttf").to_vec());
            app.font_ns.insert("noto-sans".into(), 0);
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
        let len = self.fonts.len();
        match (default, len) {
            (false, _) => self.fonts.push(data),
            (true, 0) => self.fonts.push(data),
            (true, _) => self.fonts[0] = data,
        };
        let index = match default {
            true => 0,
            false => len,
        };
        self.font_ns.insert(name, index);
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
                    if let Some(mut node) = self.kidnap_node(&path) {
                        node.set_focused(false);
                        self.restore_node(&path, node).unwrap();
                    }
                    if let None = path.pop() {
                        break;
                    }
                }
            }
            if let Some((_, mut path)) = self.focus.clone() {
                loop {
                    if let Some(mut node) = self.kidnap_node(&path) {
                        node.set_focused(true);
                        self.restore_node(&path, node).unwrap();
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
                    let event_mask = node.supported_events();
                    if event_mask.contains(event.event_type()) {
                        let mut node = self.kidnap_node(&path).unwrap();
                        let result = node.handle(self, &path, event)?;
                        let _ = self.restore_node(&path, node);
                        break result;
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
        if let Some(view) = self.view.as_ref() {
            let min = Point::zero();
            let size = view.get_spot_size();
            if !Self::hit_test_for(view, min, size, point, &mut path) {
                path.clear()
            }
        }
        path
    }

    fn hit_test_for(
        node: &NodeBox,
        min: Point,
        size: Size,
        p: Point,
        path: &mut NodePath,
    ) -> bool {
        let max_x = min.x + size.w as isize;
        let max_y = min.y + size.h as isize;
        if (min.x..max_x).contains(&p.x) && (min.y..max_y).contains(&p.y) {
            if let Some(mut cursor) = node.cursor(min) {
                let children = node.children();
                for i in 0..children.len() {
                    path.push(i);
                    if let Some(child) = children[i].as_ref() {
                        let (min, size, _) = cursor.advance(child);
                        if Self::hit_test_for(child, min, size, p, path) {
                            return true;
                        }
                    }
                    path.pop();
                }
            }
            return true;
        }
        return false;
    }

    /// The platforms can use this method to set a visual theme.
    /// They can call it again between calls to
    /// [`Application::render`] to change the theme.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        // self.global_repaint = NeedsRepaint::all();
    }

    /// Use this method to find a node in the view based
    /// on its path.
    pub fn get_node(&self, path: NodePathSlice) -> Option<&NodeBox> {
        let mut node = &self.view;
        for i in path {
            node = node.as_ref()?.children().get(*i)?;
        }
        node.as_ref()
    }

    pub fn kidnap_node(&mut self, path: NodePathSlice) -> Option<NodeBox> {
        let mut node = &mut self.view;
        for i in path {
            node = node.as_mut()?.children_mut().get_mut(*i)?;
        }
        let mut result = None;
        swap(&mut result, node);
        result
    }

    pub fn replace_kidnapped(
        &mut self,
        path: NodePathSlice,
        replacement: NodeBox,
    ) {
        self.restore_node(path, replacement).expect("Node has not been kidnapped.");
        let mut path = path.to_vec();
        self.initialize_node(&mut path).unwrap();
        self.should_recompute = true;
    }

    pub fn restore_node(&mut self, path: NodePathSlice, kidnapped: NodeBox) -> Result<(), ()> {
        let mut node = &mut self.view;
        for i in path {
            node = node.as_mut().ok_or(())?.children_mut().get_mut(*i).ok_or(())?;
        }
        if node.is_none() {
            let mut result = Some(kidnapped);
            swap(&mut result, node);
            Ok(())
        } else {
            Err(())
        }
    }

    /// Platforms should use this method to set the position
    /// and size of the view in the output buffer.
    ///
    /// Returns `true` if the previous and new values differ.
    pub fn set_fb_size(&mut self, size: Size) -> bool {
        if self.fb_size != size {
            self.fb_size = size;
            self.should_recompute = true;
            // self.global_repaint = NeedsRepaint::BACKGROUND | NeedsRepaint::OVERLAY;
            true
        } else {
            false
        }
    }

    pub fn for_each_node<T, U: Copy>(
        &self,
        path: &mut NodePath,
        arg1: &T,
        arg2: U,
        f: impl Copy + Fn(&NodeBox, &T, U, NodePathSlice) -> U,
    ) {
        let node = self.get_node(path).expect("invalid path");
        let arg2 = f(node, arg1, arg2, path);
        let children = node.children().len();
        for i in 0..children {
            path.push(i);
            self.for_each_node(path, arg1, arg2, f);
            path.pop();
        }
    }

    pub fn for_each_kidnapped_node(
        &mut self,
        path: &mut NodePath,
        f: impl Copy + Fn(&mut NodeBox, NodePathSlice),
    ) {
        let mut node = self.kidnap_node(path).expect("invalid path");
        f(&mut node, path);
        let children = node.children().len();
        self.restore_node(path, node).unwrap();
        for i in 0..children {
            path.push(i);
            self.for_each_kidnapped_node(path, f);
            path.pop();
        }
    }

    fn render_node_layer(
        &mut self,
        spot: &mut Spot,
        scratch: ScratchBuffer,
        path: &mut NodePath,
        style: usize,
        layer: RenderLayer,
    ) -> Result<(), ()> {
        let mut node = status(self.kidnap_node(path)).unwrap();
        if layer.cached(node.layers_to_cache()) {
            let mut cache = match node.restore_cache(layer) {
                Some(cache) => cache,
                None => Vec::new(),
            };
            {
                let (_, size, margin) = spot.window;
                cache.resize(size.w * size.h * RGBA, 0);
                let mut tmp_spot = Spot {
                    window: (Point::zero(), size, margin),
                    framebuffer: cache.as_mut_slice(),
                    fb_size: size,
                };
                node.render(layer, self, path, style, &mut tmp_spot, scratch).unwrap();
            }
            spot.blit(&cache, false);
            if let Err(()) = node.store_cache(layer, cache) {
                panic!("{} does not implement Node::render_cache", node.describe());
            }
        } else {
            node.render(layer, self, path, style, spot, scratch).unwrap();
        }
        self.restore_node(path, node).unwrap();
        Ok(())
    }

    fn render_node(
        &mut self,
        spot: &mut Spot,
        scratch: ScratchBuffer,
        path: &mut NodePath,
        style: usize,
    ) -> Result<(), ()> {
        let node = status(self.get_node(path)).unwrap();
        if let Some((top_left, _)) = spot.inner_crop(true) {
            if let Some(mut cursor) = node.cursor(top_left) {
                let backup = spot.window;

                self.render_node_layer(spot, scratch, path, style, RenderLayer::Background).unwrap();

                let node = self.get_node(path).unwrap();
                let children = node.children().len();
                let style_ovrd = node.style_override().unwrap_or(style);

                for i in 0..children {
                    path.push(i);

                    let child = status(self.get_node(path)).unwrap();
                    spot.set_window(cursor.advance(child));
                    self.render_node(spot, scratch, path, style_ovrd).unwrap();

                    path.pop();
                }

                spot.set_window(backup);
            }
        }

        self.render_node_layer(spot, scratch, path, style, RenderLayer::Foreground).unwrap();

        Ok(())
    }

    fn tick_node(
        &mut self,
        scratch: ScratchBuffer,
        path: &mut NodePath,
        style: usize,
    ) -> Result<bool, ()> {
        let mut node = status(self.kidnap_node(path)).unwrap();

        let mut dirty = node.tick(self, path, style, scratch).unwrap();

        let style = node.style_override().unwrap_or(style);
        let children = node.children().len();

        self.restore_node(path, node).unwrap();

        for i in 0..children {
            path.push(i);
            dirty |= self.tick_node(scratch, path, style).unwrap();
            path.pop();
        }

        Ok(dirty)
    }

    /// This method is called by the platform to request a refresh
    /// of the output. It should be called for every frame.
    pub fn render(&mut self, spot: &mut Spot, scratch: ScratchBuffer) {
        let mut path = Vec::new();
        let mut count = 0;
        while count < 5 {
            if self.should_recompute {
                self.log("recomputing layout");
                let fb_size = self.fb_size;
                if let Some(view) = self.view.as_mut() {
                    let mut sizes = Vec::new();
                    view.push_spot_sizes(&mut sizes);
                    view.set_spot_size(fb_size);
                    compute_tree(view).unwrap();
                    view.detect_size_changes(&mut sizes);
                }

                /*if let Some(view) = self.view.as_ref() {
                    view.tree_log(self, 0);
                }*/

                path.clear();
                self.should_recompute = false;
            } else if count > 0 {
                break;
            }

            if self.tick_node(scratch, &mut path, 0).unwrap() {
                // self.log("render");
                spot.fill([0; RGBA], false);
                self.render_node(spot, scratch, &mut path, 0).unwrap();
            }

            count += 1;
        }
    }

    pub fn data_response(&mut self, request: usize, data: &[u8]) -> Result<(), ()> {
        let request = self.data_requests.swap_remove(request);
        let mut node = self.kidnap_node(&request.node).unwrap();
        let result = node.loaded(self, &request.node, &request.name, 0, data);
        let _ = self.restore_node(&request.node, node);
        result
    }

    pub fn initialize_node(&mut self, path: &mut NodePath) -> Result<(), String> {
        if let Some(mut node) = self.kidnap_node(path) {
            node.initialize(self, path)?;
            let children = node.children().len();
            if let Err(()) = self.restore_node(path, node) {
                self.log("node has replaced itself");
                return Ok(());
            }
            for i in 0..children {
                path.push(i);
                self.initialize_node(path)?;
                path.pop();
            }
            Ok(())
        } else {
            Err(format!("App::initialize_node: invalid path ({:?})", path))
        }
    }

    /// Anyone can use this method to log messages.
    pub fn log(&self, message: &str) {
        (self.platform_log)(message)
    }
}
