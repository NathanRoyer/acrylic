use super::xml::{XmlNodeTree, XML_MUTATOR};
use super::visual::{Pixels, SignedPixels, Position, Size, write_framebuffer, constrain, Texture as _};
use super::event::Event;
use super::state::{StateValue, StatePathHash, StateMasks, StateFinder, StateFinderResult, StatePathStep, path_steps};
use super::node::{NodeTree, NodeKey};
use super::layout::compute_layout;
use super::style::Theme;
use super::rgb::RGBA8;
use super::glyph::FONT_MUTATOR;
use crate::builtin::png::PNG_MUTATOR;
use crate::builtin::container::{INF_MUTATOR, CONTAINERS};
use crate::builtin::textual::{LABEL_MUTATOR, PARAGRAPH_MUTATOR};
use oakwood::{index, NodeKey as _};
use crate::{Error, error, String, CheapString, Vec, Box, Rc, Hasher, HashMap};
use core::{time::Duration, ops::Deref, hash::Hasher as _, mem::replace, any::Any};
use super::for_each_child;

index!(MutatorIndex, OptionalMutatorIndex);

pub type Handler = fn(&mut Application, MutatorIndex, Event) -> Result<(), Error>;
pub type Storage = Vec<Option<Box<dyn Any>>>;

#[derive(Clone)]
pub struct Mutator {
    pub xml_tag: Option<CheapString>,
    pub xml_attr_set: Option<&'static [&'static str]>,
    pub xml_accepts_children: bool,
    pub handler: Handler,
}

struct Request {
    asset: CheapString,
    parse: bool,
    origin: NodeKey,
}

enum Asset {
    Parsed,
    Raw(Rc<[u8]>),
}

pub struct Application {
    pub root: NodeKey,
    pub view: NodeTree,
    pub storage: Storage,
    pub xml_tree: XmlNodeTree,
    pub theme: Theme,
    pub(crate) state_masks: StateMasks,
    state: StateValue,
    pub(crate) monitors: HashMap<StatePathHash, Vec<NodeKey>>,
    pub(crate) mutators: Vec<Mutator>,
    must_check_layout: bool,
    _source_files: Vec<String>,
    pub(crate) default_font_str: CheapString,
    _age: Duration,
    render_list: Vec<(Position, Size)>,
    debugged: Option<NodeKey>,

    assets: HashMap<CheapString, Asset>,
    requests: Vec<Request>,
}

pub fn get_storage<T: Any>(storage: &mut [Option<Box<dyn Any>>], m: MutatorIndex) -> Option<&mut T> {
    storage[usize::from(m)].as_mut()?.downcast_mut()
}

pub const XML_MUTATOR_INDEX: usize = 0;
pub const FONT_MUTATOR_INDEX: usize = 1;

impl Application {
    pub fn new<const N: usize>(
        layout_asset: CheapString,
        addon_mutators: [Mutator; N],
    ) -> Self {
        let default_mutators = &[
            XML_MUTATOR,
            FONT_MUTATOR,
            PNG_MUTATOR,
            LABEL_MUTATOR,
            PARAGRAPH_MUTATOR,
            INF_MUTATOR,
        ];

        let cap = default_mutators.len() + CONTAINERS.len() + addon_mutators.len();
        let mut mutators = Vec::with_capacity(cap);
        mutators.extend_from_slice(default_mutators);
        mutators.extend_from_slice(&CONTAINERS);
        mutators.extend(addon_mutators.into_iter());

        let storage = mutators.iter().map(|_| None as Option<Box<dyn Any>>).collect();

        let mut app = Self {
            root: Default::default(),
            view: NodeTree::new(),
            xml_tree: XmlNodeTree::new(),
            state: super::state::parse_state(include_str!("default.json")).unwrap(),
            state_masks: Default::default(),
            monitors: HashMap::new(),
            mutators,
            storage,
            must_check_layout: false,
            _source_files: Vec::new(),
            default_font_str: "default-font".into(),
            theme: Theme::parse(include_str!("default-theme.json")).unwrap(),
            _age: Duration::from_secs(0),
            render_list: Vec::new(),
            debugged: None,

            assets: HashMap::new(),
            requests: Vec::new(),
        };

        for i in 0..app.mutators.len() {
            let handle = app.mutators[i].handler;
            handle(&mut app, i.into(), Event::Initialize).unwrap();
        }

        if true {
            let default_font = crate::NOTO_SANS.to_vec().into_boxed_slice();
            app.mutate(FONT_MUTATOR_INDEX.into(), Event::ParseAsset {
                node_key: Default::default(),
                asset: app.default_font_str.clone(),
                bytes: default_font,
            }).unwrap();
            app.assets.insert(app.default_font_str.clone(), Asset::Parsed);
        }

        let factory = Some(XML_MUTATOR_INDEX.into()).into();

        let xml_root = app.xml_tree.create();
        app.xml_tree[xml_root].factory = factory;
        app.xml_tree[xml_root].attributes.push("file", layout_asset.clone());

        app.root = app.view.create();
        app.view[app.root].factory = factory;
        app.view[app.root].xml_node_index = Some(xml_root.index().into()).into();

        app.request(layout_asset, app.root, false).unwrap();

        app
    }

    pub fn invalidate_layout(&mut self) {
        self.must_check_layout = true;
    }

    pub fn mutate(&mut self, index: MutatorIndex, event: Event) -> Result<(), Error> {
        let mutator = &self.mutators[usize::from(index)];
        /*let xml_tag = mutator.xml_tag.clone().unwrap_or("<anon>".into());
        log::info!("{}: handling {}", xml_tag, &event);*/
        (mutator.handler)(self, index, event)
    }

    pub fn handle(&mut self, node: NodeKey, event: Event) -> Result<Option<()>, Error> {
        if Some(node) == self.debugged {
            log::info!("handling {}", &event);
        }

        Ok(match self.view[node].factory.get() {
            Some(index) => Some(self.mutate(index, event)?),
            None => None,
        })
    }

    pub fn get_asset(&self, asset: &CheapString) -> Result<Rc<[u8]>, Error> {
        match self.assets.get(asset) {
            Some(Asset::Raw(rc)) => Ok(rc.clone()),
            Some(Asset::Parsed) => Err(error!("Asset {} was stored in mutator storage", asset.deref())),
            None => Err(error!("Asset {} was not found", asset.deref())),
        }
    }

    pub fn requested(&self) -> Option<CheapString> {
        self.requests.first().map(|r| r.asset.clone())
    }

    /// If `asset` is already loaded, this will trigger
    /// Handling of an `AssetLoaded` event immediately
    pub fn request(&mut self, asset: CheapString, origin: NodeKey, parse: bool) -> Result<(), Error> {
        if self.assets.contains_key(&asset) {
            let illegal = match (parse, &self.assets[&asset]) {
                (true, Asset::Raw(_)) => true,
                (false, Asset::Parsed) => true,
                _ => false,
            };

            if illegal {
                return Err(error!("Asset {} was previously loaded with a different `parse` flag", asset.deref()));
            }

            self.handle(origin, Event::AssetLoaded {
                node_key: origin,
            })?.ok_or_else(|| error!())
        } else {
            self.requests.push(Request {
                asset,
                origin,
                parse,
            });
            Ok(())
        }
    }

    pub fn data_response(&mut self, asset: CheapString, data: Box<[u8]>) -> Result<(), Error> {
        let mut data = Some(data);

        let mut i = 0;
        while let Some(request) = self.requests.get(i) {
            if request.asset == asset {
                let request = self.requests.swap_remove(i);
                let node_key = request.origin;

                if let Some(data) = data.take() {
                    let result = if request.parse {
                        self.handle(node_key, Event::ParseAsset {
                            node_key,
                            asset: asset.clone(),
                            bytes: data,
                        })?;

                        Asset::Parsed
                    } else {
                        Asset::Raw(data.into())
                    };

                    self.assets.insert(asset.clone(), result);
                }

                self.request(asset.clone(), request.origin, request.parse)?;
            } else {
                i += 1;
            }
        }

        Ok(())
    }

    pub fn subscribe_to_state(&mut self, node: NodeKey, path_hash: StatePathHash) {
        if let Some(subscribed) = self.monitors.get_mut(&path_hash) {
            if !subscribed.contains(&node) {
                subscribed.push(node);
            }
        } else {
            let mut subscribed = Vec::with_capacity(1);
            subscribed.push(node);
            self.monitors.insert(path_hash, subscribed);
        }
    }

    pub fn state_lookup<'a>(&'a mut self, node: NodeKey, store: &str, key: &str, path_hash: &mut Hasher) -> Result<&'a mut StateValue, Error> {
        let mut state_finder: Option<(StateFinder, NodeKey)> = None;

        let mut target = node;
        while let Some(parent) = self.view.parent(target) {
            if let Some(finder) = self.state_masks.get(&parent) {
                state_finder = Some((*finder, parent));
                break;
            } else {
                target = parent;
            }
        }

        if let Some((finder, masker)) = state_finder {
            finder(self, masker, node, store, key, path_hash)
        } else {
            if store == "root" {
                let mut current = &mut self.state;
                for path_step in path_steps(key) {
                    let option = match path_step {
                        StatePathStep::Index(index) => {
                            path_hash.write_usize(index);
                            current.get_mut(index)
                        },
                        StatePathStep::Key(key) => {
                            path_hash.write(key.as_bytes());
                            current.get_mut(key)
                        },
                    };

                    current = match option {
                        Some(value) => value,
                        None => return Err(error!("Invalid state key: {}", key)),
                    }
                }
                Ok(current)
            } else {
                Err(error!("Unknown state store: {}", store))
            }
        }
    }

    pub fn state_update(&mut self, path_scope: NodeKey, store: &str, key: &str, value: StateValue) -> Result<(), Error> {
        let mut path_hash = Hasher::default();
        let content = self.state_lookup(path_scope, store, key, &mut path_hash)?;
        *content = value;
        let path_hash = path_hash.finish();

        if let Some(subscribed) = self.monitors.get_mut(&path_hash) {
            for node_key in replace(subscribed, Vec::new()) {
                if let Some(_) = self.view.get(node_key) {
                    let xml_node_index = self.view[node_key].xml_node_index;
                    let factory = self.view[node_key].factory;
                    self.view.reset(node_key);
                    self.view[node_key].xml_node_index = xml_node_index;
                    self.view[node_key].factory = factory;

                    if let Some(index) = xml_node_index.get() {
                        let xml_node_key = self.xml_tree.node_key(index);
                        self.handle(node_key, Event::Populate {
                            node_key,
                            xml_node_key,
                        })
                    } else {
                        Err(error!("Non-XML nodes cannot subscribe to state updates"))
                    }?;
                }
            }
        }

        Ok(())
    }

    pub fn xml_tag(&self, node: NodeKey) -> CheapString {
        let mutator_index = match self.view[node].factory.get() {
            Some(index) => index,
            None => return "<subnode>".into(),
        };
        let mutator = &self.mutators[usize::from(mutator_index)];
        match &mutator.xml_tag {
            Some(cs) => cs.clone(),
            None => "<anon>".into(),
        }
    }

    pub fn attr(&mut self, node: NodeKey, attr: &str, default: Option<CheapString>) -> Result<StateFinderResult, Error> {
        let xml_node_index = self.view[node].xml_node_index.get()
            .expect("cannot use Application::attr on nodes without xml_node_index");
        let xml_node_key = self.xml_tree.node_key(xml_node_index);
        let xml_node = &self.xml_tree[xml_node_key];

        let alen = attr.len();

        let mut found = None;
        for (key, value) in xml_node.attributes.iter() {
            if key.deref() == attr {
                return Ok(StateFinderResult::String(value.clone()));
            } else if key.starts_with(attr) && key.get(alen..alen + 1) == Some(&":") {
                found = Some((key.clone(), value.clone()))
            }
        }

        if let Some((key, value)) = found {
            let mut path_hash = Hasher::default();
            let store = &key[alen + 1..];
            let value = self.state_lookup(node, store, value.deref(), &mut path_hash)?;
            let path_hash = path_hash.finish();

            let retval = match value {
                StateValue::Null => Err(error!("State value is null: {}", key)),
                StateValue::Array(_) => Err(error!("State value isn't primitive: : {}", key)),
                StateValue::Object(_) => Err(error!("State value isn't primitive: : {}", key)),

                StateValue::Bool(b) => Ok(StateFinderResult::Boolean(*b)),
                StateValue::Number(_) => Ok(StateFinderResult::Number(value.as_f64().unwrap() as _)),
                StateValue::String(s) => Ok(StateFinderResult::String(s.clone().into())),
            };

            self.subscribe_to_state(node, path_hash);

            retval
        } else {
            match default {
                Some(default) => Ok(StateFinderResult::String(default)),
                None => Err(error!("Missing {:?} attribute on <{}>", attr, self.xml_tag(node).deref())),
            }
        }
    }

    fn build_render_list(&mut self, fb_rect: &(Position, Size), key: NodeKey, querying: bool) {
        let _tag = self.xml_tag(key);
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
            self.view[key].background.paint(fb, texture_coords, sampling_window, stride, 3, true);
        }

        for_each_child!(self.view, key, child, {
            self.paint(child, fb, stride, restrict)?;
        });


        for sampling_window in &self.render_list {
            let mut sampling_window = *sampling_window;
            constrain(&texture_coords, &mut sampling_window);
            constrain(restrict, &mut sampling_window);
            // super::visual::debug_framebuffer(fb, stride, sampling_window);

            self.view[key].foreground.paint(fb, texture_coords, sampling_window, stride, 3, true);
        }

        *restrict = backup;

        Ok(())
    }

    pub fn render(
        &mut self,
        fb_size: (usize, usize),
        framebuffer: &mut [RGBA8],
        mx: usize,
        my: usize,
        wheel_delta: isize,
        _click: bool,
    ) -> Result<&[(Position, Size)], Error> {
        // log::info!("frame");
        let stride = fb_size.0;
        let new_size = Size::new(Pixels::from_num(stride), Pixels::from_num(fb_size.1));
        let _mouse = Position::new(SignedPixels::from_num(mx), SignedPixels::from_num(my));

        /*if false {
            let s = crate::format!("mouse: {}x{}", mx, my);
            self.state_update(self.root, "root", "test.3", StateValue::String(s))?;
        }

        if _click {
            let debugged = super::layout::hit_test(&mut self.view, self.root, _mouse);
            log::info!("debugged: {}", self.xml_tag(debugged));
            self.debugged = Some(debugged);
        }*/

        if _click {
            let mut path_hash = Hasher::default();
            let mut array = self.state_lookup(self.root, "root", "test", &mut path_hash).unwrap().clone();
            if let Some(array) = array.as_array_mut() {
                array.push(StateValue::String(crate::format!("DAMN {}", array.len())));
            }
            self.state_update(self.root, "root", "test", array).unwrap();
        }

        if wheel_delta != 0 {
            let mut node = super::layout::hit_test(&mut self.view, self.root, _mouse);
            loop {
                let (axis, scroll, max_scroll) = super::layout::get_scroll(self, node);
                if max_scroll.is_none() {
                    if let Some(parent) = self.view.parent(node) {
                        node = parent;
                        continue;
                    } else {
                        break;
                    }
                }

                let scroll = scroll.unwrap_or(SignedPixels::ZERO);
                let max_scroll = max_scroll.unwrap().to_num::<SignedPixels>();
                log::info!("{:?}, {:?}, {:?}", axis, scroll, max_scroll);
                let mut candidate = SignedPixels::from_num(wheel_delta);

                let new_scroll = scroll - candidate;
                if new_scroll > max_scroll {
                    candidate = scroll - max_scroll;
                } else if new_scroll < SignedPixels::ZERO {
                    candidate = scroll;
                }

                self.view[node].layout_config.set_dirty(true);
                super::layout::scroll(self, node, axis, candidate);
                break;
            }
        }

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

        if self.must_check_layout {
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
