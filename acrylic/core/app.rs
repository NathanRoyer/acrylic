//! The state of your Application

use super::xml::{XmlNodeTree, XmlNodeKey, AttributeValue, AttributeValueVec, AttributeValueType};
use crate::{Error, error, String, ArcStr, Vec, Box, Rc, HashMap, LiteMap, DEFAULT_FONT_NAME};
use super::visual::{Pixels, Position, Size, write_framebuffer, constrain, Texture as _};
use super::style::{Theme, Style, DEFAULT_STYLE};
use super::layout::{compute_layout, hit_test};
use super::node::{NodeTree, NodeKey, Mutator};
use super::state::{Namespace, root_ns};
use core::{time::Duration, ops::Deref};
use super::event::UserInputEvent;
use super::text_edit::Cursor;
use super::for_each_child;
use super::rgb::RGBA8;

use oakwood::{NodeKey as _};
use lmfu::json::{JsonFile, Value, Path, parse_path};

use super::glyph::FONT_MUTATOR;

use crate::builtin::{
    inflate::INFLATE_MUTATOR,
    import::IMPORT_MUTATOR,
    png::PNG_MUTATOR,
    container::CONTAINERS,
    label::LABEL_MUTATOR,
    paragraph::{PARAGRAPH_MUTATOR, UNBREAKABLE_MUTATOR},
};

#[cfg(doc)]
use super::node::Node;

/// General-purpose callbacks that containers can call based on their attributes.
pub type SimpleCallback = fn(&mut Application, NodeKey) -> Result<(), Error>;

/// General-purpose callbacks that containers can call based on their attributes.
pub type SimpleCallbackMap = HashMap<ArcStr, SimpleCallback>;

struct Request {
    asset: ArcStr,
    parse: bool,
    origin: NodeKey,
}

enum Asset {
    Parsed,
    Raw(Rc<[u8]>),
}

pub struct DebuggingOptions {
    pub skip_glyph_rendering: bool,
    pub skip_container_borders: bool,
    pub freeze_layout: bool,
    pub draw_layout: bool,
}

/// A Singleton which represents your application.
///
/// Its content includes:
/// - the list of [`Mutator`]s
/// - the XML layout
/// - the JSON state and related triggers
/// - the internal view representation (a Node tree)
/// - the [`Theme`]
/// - a cache of assets
pub struct Application {
    pub root: NodeKey,
    pub view: NodeTree,
    pub xml_tree: XmlNodeTree,
    pub theme: Theme,
    pub callbacks: SimpleCallbackMap,
    pub debug: DebuggingOptions,
    pub state: JsonFile,

    pub(crate) namespaces: LiteMap<NodeKey, Namespace>,
    pub(crate) mutators: Vec<Mutator>,
    pub(crate) text_cursors: Vec<Cursor>,

    focus_coords: Position,
    focused: Option<NodeKey>,
    must_check_layout: bool,
    _source_files: Vec<String>,
    _age: Duration,
    render_list: Vec<(Position, Size)>,
    assets: HashMap<ArcStr, Asset>,
    requests: Vec<Request>,
}

pub const IMPORT_MUTATOR_INDEX: usize = 0;
pub const FONT_MUTATOR_INDEX: usize = 1;
pub const UNBREAKABLE_MUTATOR_INDEX: usize = 5;

impl Application {
    /// Main constructor
    pub fn new(layout_asset: ArcStr, callbacks: SimpleCallbackMap) -> Self {
        let default_mutators = &[
            IMPORT_MUTATOR,
            FONT_MUTATOR,
            PNG_MUTATOR,
            LABEL_MUTATOR,
            PARAGRAPH_MUTATOR,
            UNBREAKABLE_MUTATOR,
            INFLATE_MUTATOR,
        ];

        assert_eq!(default_mutators[IMPORT_MUTATOR_INDEX].name, "ImportMutator");
        assert_eq!(default_mutators[FONT_MUTATOR_INDEX].name, "FontMutator");
        assert_eq!(default_mutators[UNBREAKABLE_MUTATOR_INDEX].name, "UnbreakableMutator");

        let mut mutators = Vec::with_capacity(default_mutators.len() + CONTAINERS.len());
        mutators.extend_from_slice(default_mutators);
        mutators.extend_from_slice(&CONTAINERS);

        let mut app = Self {
            root: Default::default(),
            view: NodeTree::new(),
            xml_tree: XmlNodeTree::new(),
            state: JsonFile::new(Some(include_str!("default.json"))).unwrap(),
            namespaces: LiteMap::new(),
            // monitors: LiteMap::new(),
            callbacks,
            mutators,
            must_check_layout: false,
            _source_files: Vec::new(),
            text_cursors: Vec::new(),
            focus_coords: Position::zero(),
            focused: None,
            theme: Theme::parse(include_str!("default-theme.json")).unwrap(),
            _age: Duration::from_secs(0),
            render_list: Vec::new(),
            debug: DebuggingOptions {
                skip_glyph_rendering: false,
                skip_container_borders: false,
                freeze_layout: false,
                draw_layout: false,
            },

            assets: HashMap::new(),
            requests: Vec::new(),
        };

        for i in 0..app.mutators.len() {
            (app.mutators[i].handlers.initializer)(&mut app, i.into()).unwrap();
        }

        if true {
            let default_font = crate::NOTO_SANS.to_vec().into_boxed_slice();
            let font_parser = app.mutators[FONT_MUTATOR_INDEX].handlers.parser;
            font_parser(
                &mut app,
                FONT_MUTATOR_INDEX.into(),
                Default::default(),
                &DEFAULT_FONT_NAME,
                default_font,
            ).unwrap();
            app.assets.insert((*DEFAULT_FONT_NAME).clone(), Asset::Parsed);
        }

        let factory = Some(IMPORT_MUTATOR_INDEX.into()).into();

        let xml_root = app.xml_tree.create();
        app.xml_tree[xml_root].factory = factory;
        app.xml_tree[xml_root].attributes = AttributeValueVec::new_import(layout_asset);

        app.root = app.view.create();
        app.view[app.root].factory = factory;
        app.view[app.root].xml_node_index = Some(xml_root.index().into()).into();

        app.namespaces.insert(app.root, root_ns());

        app.populate(app.root, xml_root).unwrap();

        app
    }

    /// Reload the view, allowing it to pick up state changes
    pub fn reload_view(&mut self) {
        let backup = &self.view[self.root];
        let factory = backup.factory;
        let xml_node_index = backup.xml_node_index;

        let focused_path = self.focused.map(|mut node_key| {
            let mut path = Vec::new();

            while let Some(parent) = self.view.parent(node_key) {
                let index = self.view.child_index(node_key).unwrap();
                path.push(index);
                node_key = parent;
            }

            path
        });

        self.view.reset(self.root);
        self.invalidate_layout();
        let root_ns = self.namespaces.remove(&self.root).unwrap();
        self.namespaces.clear();
        self.namespaces.insert(self.root, root_ns);

        let root = &mut self.view[self.root];
        root.factory = factory;
        root.xml_node_index = xml_node_index;

        if let Some(xml_root) = xml_node_index.get() {
            let xml_root = self.xml_tree.node_key(xml_root);
            self.populate(self.root, xml_root).unwrap();
        }

        self.focused = focused_path.map(|mut path| {
            let mut node_key = self.root;

            while let Some(index) = path.pop() {
                node_key = self.view.first_child(node_key).unwrap();
                for _ in 0..index {
                    node_key = self.view.next_sibling(node_key);
                }
            }

            node_key
        });
    }

    /// Quick way to tell the application to recompute its layout before the next frame
    pub fn invalidate_layout(&mut self) {
        self.must_check_layout = true;
    }

    /// Read an asset from the internal cache
    pub fn get_asset(&self, asset: &ArcStr) -> Result<Rc<[u8]>, Error> {
        match self.assets.get(asset) {
            Some(Asset::Raw(rc)) => Ok(rc.clone()),
            Some(Asset::Parsed) => Err(error!("Asset {} was stored in mutator storage", asset.deref())),
            None => Err(error!("Asset {} was not found", asset.deref())),
        }
    }

    /// Platforms use this method to read the next asset to load.
    pub fn requested(&self) -> Option<ArcStr> {
        self.requests.first().map(|r| r.asset.clone())
    }

    /// Notify the system that an asset is required by some [`Node`]
    ///
    /// If `asset` is already loaded, this will trigger
    /// Handling of an `AssetLoaded` event immediately
    pub fn request(&mut self, asset: &ArcStr, origin: NodeKey, parse: bool) -> Result<(), Error> {
        if let Some(content) = self.assets.get(&asset) {
            let illegal = match (parse, content) {
                (true, Asset::Raw(_)) => true,
                (false, Asset::Parsed) => true,
                _ => false,
            };

            if illegal {
                return Err(error!("Asset {} was previously loaded with a different `parse` flag", asset.deref()));
            }

            self.finalize(origin)
        } else {
            self.requests.push(Request {
                asset: asset.clone(),
                origin,
                parse,
            });
            Ok(())
        }
    }

    /// Platforms use this method to deliver an asset's content
    pub fn data_response(&mut self, asset: ArcStr, data: Box<[u8]>) -> Result<(), Error> {
        let mut data = Some(data);

        let mut i = 0;
        while let Some(request) = self.requests.get(i) {
            if request.asset == asset {
                let request = self.requests.swap_remove(i);
                let node_key = request.origin;

                if let Some(data) = data.take() {
                    let result = if request.parse {
                        self.parse(node_key, &asset, data)?;

                        Asset::Parsed
                    } else {
                        Asset::Raw(data.into())
                    };

                    self.assets.insert(asset.clone(), result);
                }

                self.request(&asset, request.origin, request.parse)?;
            } else {
                i += 1;
            }
        }

        Ok(())
    }

    pub fn get_inherited_style(&self, node_key: NodeKey) -> Result<Style, Error> {
        let mut parent_style = match self.theme.resolve(DEFAULT_STYLE) {
            Some(style_index) => Ok(style_index),
            None => Err(error!("Missing default style in theme")),
        }?;

        let mut current = node_key;
        while let Some(parent) = self.view.parent(current) {
            if let Some(p_style) = self.view[parent].style_override.get() {
                parent_style = p_style.into();
                break;
            } else {
                current = parent;
            }
        }

        Ok(self.theme.get(parent_style))
    }

    /// Retrieves a value from the JSON state
    pub fn resolve(
        &self,
        node: NodeKey,
        ns_name: &str,
        ns_path: &str,
    ) -> Result<Path, Error> {
        let mut target = node;
        loop {
            match self.namespaces.get(&target) {
                Some(ns) if &*ns.name == ns_name => {
                    let mut jp = ns.path.clone();
                    (ns.callback)(&self, target, node, &mut jp)?;
                    jp.append(parse_path(ns_path));
                    break Ok(jp);
                },
                _ => match self.view.parent(target) {
                    Some(parent) => target = parent,
                    None => break Err(error!("Missing {} namespace", ns_name)),
                },
            }
        }
    }

    /// Retrieves the XML tag name of a node
    ///
    /// This can return the following special strings:
    /// - `<subnode>` if the node wasn't created from an XML tag
    /// - `<anon>` if the node's [`Mutator`] has no defined XML tag
    pub fn xml_tag(&self, node: NodeKey) -> ArcStr {
        let mutator_index = match self.view[node].factory.get() {
            Some(index) => index,
            None => return "<subnode>".into(),
        };
        let mutator = &self.mutators[usize::from(mutator_index)];
        match &mutator.xml_params {
            Some(params) => params.tag_name.clone(),
            None => "<anon>".into(),
        }
    }

    #[doc(hidden)]
    pub fn attr_state_path(&mut self, node: NodeKey, attr: usize) -> Result<Result<(Path, AttributeValueType), AttributeValue>, Error> {
        let xml_node_index = self.view[node].xml_node_index.get()
            .expect("cannot use Application::attr on nodes without xml_node_index");
        let xml_node_key = self.xml_tree.node_key(xml_node_index);
        let xml_node = &self.xml_tree[xml_node_key];

        let (namespace, path, value_type) = match xml_node.attributes.get(attr).clone() {
            AttributeValue::StateLookup { namespace, path, value_type } => (namespace, path, value_type),
            value => return Ok(Err(value)),
        };

        Ok(Ok((self.resolve(node, &namespace, path.deref())?, value_type)))
    }

    /// Retrieves the value of an XML attribute, resolving optional JSON state dependencies.
    ///
    /// # Attribute syntax
    ///
    /// ## Immediate syntax
    /// 
    /// `<png file="acrylic.png" />`
    ///
    /// Here, the `file` attribute will contain the string `acrylic.png`
    ///
    /// ## JSON state dependency
    ///
    /// `<label root:text="some.json.path.items.3" />`
    ///
    /// Here, the `text` attribute will contain the value of the JSON state at `some` / `json` / `path` / `items` / fourth item.
    ///
    /// `root` specifies the main JSON state namespace. Use [Iterating Containers](http://todo.io/) to create other ones.
    pub fn attr<T: TryFrom<AttributeValue, Error=Error>>(
        &mut self,
        node: NodeKey,
        attr: usize,
    ) -> Result<T, Error> {
        use AttributeValueType::*;

        let (json_path, value_type) = match self.attr_state_path(node, attr)? {
            Ok(tuple) => tuple,
            Err(value) => return T::try_from(value),
        };

        let value = match (&self.state[&json_path], value_type) {
            // String dumps:
            (
                Value::Array  (_) |
                Value::Object (_) |
                Value::Number (_) |
                Value::Boolean(_) |
                Value::Null,

                Other | OptOther,
            ) => {
                let string = self.state.dump(&json_path)
                    .map_err(|e| error!("unexpected fmt error: {:?}", e))?;

                match value_type {
                    OptOther => AttributeValue::OptOther(Some(string)),
                    Other => AttributeValue::Other(string),
                    _ => unreachable!(),
                }
            },

            // Common conversions:
            (Value::String(s), _) => AttributeValue::parse(&s, value_type)?,

            _ => return Err(error!("Invalid Attribute Conversion")),
        };

        // self.subscribe_to_state(node, json_path);

        T::try_from(value)
    }

    fn build_render_list(&mut self, fb_rect: &(Position, Size), key: NodeKey, querying: bool) {
        // let _tag = self.xml_tag(key);
        let node = &mut self.view[key];
        if node.layout_config.get_dirty() {
            // log::info!("node {} ({}) is dirty", _tag, key.index());
            node.layout_config.set_dirty(false);

            if querying {
                let mut rect = (node.position, node.size);
                constrain(fb_rect, &mut rect);
                self.render_list.push(rect);
            }

            // all children that are in the container will also be re-rendered
            for_each_child!(self.view, key, child, {
                self.build_render_list(fb_rect, child, false);
            });
        } else {
            for_each_child!(self.view, key, child, {
                self.build_render_list(fb_rect, child, querying);
            });
        }
    }

    fn paint(
        &mut self,
        key: NodeKey,
        fb: &mut [RGBA8],
        stride: usize,
        restrict: &mut (Position, Size),
    ) -> Result<(), Error> {
        let size = self.view[key].size;
        let position = self.view[key].position;
        let texture_coords = (position, size);

        let backup = *restrict;
        constrain(&texture_coords, restrict);

        for sampling_window in &self.render_list {
            let mut sampling_window = *sampling_window;
            constrain(&texture_coords, &mut sampling_window);
            constrain(restrict, &mut sampling_window);
            self.view[key].background.paint(fb, texture_coords, sampling_window, stride, true, false);
        }

        for_each_child!(self.view, key, child, {
            self.paint(child, fb, stride, restrict)?;
        });


        for sampling_window in &self.render_list {
            let mut sampling_window = *sampling_window;
            constrain(&texture_coords, &mut sampling_window);
            constrain(restrict, &mut sampling_window);
            if self.debug.draw_layout {
                super::visual::debug_framebuffer(fb, stride, sampling_window);
            }

            self.view[key].foreground.paint(fb, texture_coords, sampling_window, stride, true, false);
        }

        *restrict = backup;

        Ok(())
    }

    /// Alias of [`hit_test`]
    pub fn hit_test(&self, position: Position) -> NodeKey {
        hit_test(&self.view, self.root, position)
    }

    pub fn get_focus_coords(&self) -> Position {
        self.focus_coords
    }

    pub fn set_focus(&mut self, focus_coords: Position) -> NodeKey {
        self.focus_coords = focus_coords;
        self.hit_test(focus_coords)
    }

    pub fn set_focused_node(&mut self, node_key: NodeKey) -> Result<(), Error> {
        self.clear_focused_node()?;
        self.focused = Some(node_key);

        Ok(())
    }

    pub fn clear_focused_node(&mut self) -> Result<(), Error> {
        if let Some(node_key) = self.focused {
            let input_event = UserInputEvent::FocusLoss;
            self.handle_user_input(node_key, &input_event)?;
            self.focused = None;
        }

        Ok(())
    }

    pub fn get_focused_node_or_at(&mut self, focus_coords: Position) -> NodeKey {
        let under_focus_coords = self.set_focus(focus_coords);
        match self.focused {
            Some(node_key) => node_key,
            None => under_focus_coords,
        }
    }

    pub fn get_focused_node(&mut self) -> Option<NodeKey> {
        self.focused
    }

    /// Renders the current view in a `framebuffer`.
    ///
    /// This expects the framebuffer to keep its content between calls.
    ///
    /// TODO: remove temporary input code from this and implement Input Events.
    ///
    /// This methods follows the following steps:
    /// - If the framebuffer size changed: invalidate layout & empty framebuffer.
    /// - Recompute the layout if needed.
    /// - Builds a list of dirty rectangles by locating each dirty node in the view
    /// - repaint each rectangle in this list
    pub fn render(
        &mut self,
        fb_size: (usize, usize),
        framebuffer: &mut [RGBA8],
    ) -> Result<&[(Position, Size)], Error> {
        // log::info!("frame");
        let stride = fb_size.0;
        let new_size = Size::new(Pixels::from_num(stride), Pixels::from_num(fb_size.1));

        /*
        let node_key = super::layout::hit_test(&mut self.view, self.root, _mouse);
        let input_event = super::event::UserInputEvent::WheelY(SignedPixels::from_num(wheel_delta));

        self.handle_user_input(node_key, &input_event).unwrap();
        */

        let transparent = RGBA8::new(0, 0, 0, 0);
        let fb_rect = (Position::zero(), new_size);
        self.render_list.clear();

        if self.view[self.root].size != new_size {
            log::info!("fbsize: {}x{}", stride, fb_size.1);
            self.view[self.root].size = new_size;
            self.must_check_layout = true;
            framebuffer.fill(transparent);
            self.render_list.push(fb_rect);
        }

        if self.must_check_layout && !self.debug.freeze_layout {
            log::warn!("recomputing layout");
            compute_layout(self, self.root)?;
            self.must_check_layout = false;
        }

        if self.render_list.len() == 0 {
            self.build_render_list(&fb_rect, self.root, true);
            let mut dirty_pixels = 0;

            for rect in &self.render_list {
                dirty_pixels += (rect.1.w * rect.1.h).to_num::<usize>();
                write_framebuffer(framebuffer, stride, *rect, transparent);
            }

            if self.render_list.len() > 0 {
                log::warn!("{} repaints, total: {} pixels", self.render_list.len(), dirty_pixels);
            }
        }

        if self.render_list.len() > 0 {
            let mut restrict = fb_rect;
            self.paint(self.root, framebuffer, stride, &mut restrict)?;
        }

        Ok(&self.render_list)
    }
}

impl Application {
    pub fn parse(&mut self, node_key: NodeKey, asset: &ArcStr, bytes: Box<[u8]>) -> Result<(), Error> {
        match self.view[node_key].factory.get() {
            Some(i) => (self.mutators[usize::from(i)].handlers.parser)(self, i, node_key, asset, bytes),
            None => Err(error!("Node {:?} cannot parse: it has no factory", node_key)),
        }
    }

    pub fn populate(&mut self, node_key: NodeKey, xml_node_key: XmlNodeKey) -> Result<(), Error> {
        match self.view[node_key].factory.get() {
            Some(i) => (self.mutators[usize::from(i)].handlers.populator)(self, i, node_key, xml_node_key),
            None => Err(error!("Node {:?} cannot parse: it has no factory", node_key)),
        }
    }

    pub fn finalize(&mut self, node_key: NodeKey) -> Result<(), Error> {
        match self.view[node_key].factory.get() {
            Some(i) => (self.mutators[usize::from(i)].handlers.finalizer)(self, i, node_key),
            None => Err(error!("Node {:?} cannot parse: it has no factory", node_key)),
        }
    }

    pub fn resize(&mut self, node_key: NodeKey) -> Result<(), Error> {
        match self.view[node_key].factory.get() {
            Some(i) => (self.mutators[usize::from(i)].handlers.resizer)(self, i, node_key),
            None => Err(error!("Node {:?} cannot parse: it has no factory", node_key)),
        }
    }

    pub fn handle_user_input(&mut self, target: NodeKey, event: &UserInputEvent) -> Result<bool, Error> {
        let mut node_key = target;
        loop {
            match self.view[node_key].factory.get() {
                Some(i) => {
                    let handler = self.mutators[usize::from(i)].handlers.user_input_handler;
                    if handler(self, i, node_key, target, event)? {
                        break Ok(true);
                    } else if let Some(parent) = self.view.parent(node_key) {
                        node_key = parent;
                    } else {
                        break Ok(false);
                    }
                },
                None => break Err(error!("Node {:?} cannot parse: it has no factory", node_key)),
            }
        }
    }
}
