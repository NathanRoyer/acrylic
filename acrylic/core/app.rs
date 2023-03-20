use super::xml::{XmlNodeTree, XML_MUTATOR};
use super::visual::{Pixels, SignedPixels, Position, Size, write_framebuffer, constrain, Texture as _};
use super::event::Event;
use super::state::{StateValue, StatePathHash, StateMasks, StateFinder, StateFinderResult, path_steps};
use super::node::{NodeTree, NodeKey};
use super::layout::{compute_layout};
use super::style::Theme;
use super::rgb::RGBA8;
use crate::builtin::png::PNG_MUTATOR;
use crate::builtin::container::{INF_MUTATOR, CONTAINERS};
use crate::builtin::textual::{LABEL_MUTATOR, PARAGRAPH_MUTATOR};
use oakwood::{index, NodeKey as _};
use hashbrown::HashMap;
use crate::{Error, error, String, CheapString, Vec, Box, Rc, Hasher};
use core::{time::Duration, ops::Deref, hash::Hasher as _, mem::replace};
use super::for_each_child;

index!(MutatorIndex, OptionalMutatorIndex);

pub type Handler = fn(&mut Application, Event) -> Result<(), Error>;

#[derive(Clone)]
pub struct Mutator {
    pub xml_tag: Option<CheapString>,
    pub xml_attr_set: Option<&'static [&'static str]>,
    pub xml_accepts_children: bool,
    pub handler: Handler,
    pub storage: Option<Box<(/* Todo: storage trait: Any + Clone */)>>,
}

struct Request {
    asset: CheapString,
    origin: NodeKey,
}

pub struct Application {
    pub root: NodeKey,
    pub view: NodeTree,
    pub xml_tree: XmlNodeTree,
    pub state: StateValue,
    pub state_masks: StateMasks,
    pub monitors: HashMap<StatePathHash, Vec<NodeKey>>,
    pub mutators: Vec<Mutator>,
    pub must_check_layout: bool,
    pub source_files: Vec<String>,
    pub default_font_str: CheapString,
    pub theme: Theme,
    pub age: Duration,

    assets: HashMap<CheapString, Rc<Vec<u8>>>,
    requests: Vec<Request>,
}

impl Application {
    pub fn new(
        layout_asset: CheapString,
        addon_mutators: Vec<Mutator>,
    ) -> Self {
        let mut app = Self {
            root: Default::default(),
            view: NodeTree::new(),
            xml_tree: XmlNodeTree::new(),
            state: super::state::parse_state(include_str!("default.json")).unwrap(),
            state_masks: Default::default(),
            monitors: HashMap::new(),
            mutators: addon_mutators,
            must_check_layout: false,
            source_files: Vec::new(),
            default_font_str: "default-font".into(),
            theme: Theme::parse(include_str!("default-theme.json")).unwrap(),
            age: Duration::from_secs(0),

            assets: HashMap::new(),
            requests: Vec::new(),
        };

        app.assets.insert(app.default_font_str.clone(), Rc::new(crate::NOTO_SANS.to_vec()));

        let xml_mutator = app.mutators.len();
        let factory = Some(xml_mutator.into()).into();

        app.mutators.push(XML_MUTATOR);
        app.mutators.push(PNG_MUTATOR);
        app.mutators.push(LABEL_MUTATOR);
        app.mutators.push(PARAGRAPH_MUTATOR);
        app.mutators.push(INF_MUTATOR);
        app.mutators.extend_from_slice(&CONTAINERS);

        let xml_root = app.xml_tree.create();
        app.xml_tree[xml_root].factory = factory;
        app.xml_tree[xml_root].attributes.push("file", layout_asset.clone());

        app.root = app.view.create();
        app.view[app.root].factory = factory;
        app.view[app.root].xml_node_index = Some(xml_root.index().into()).into();

        app.request(layout_asset, app.root).unwrap();

        app
    }

    pub fn handle(&mut self, node: NodeKey, event: Event) -> Result<Option<()>, Error> {
        if let Some(index) = self.view[node].factory.get() {
            let handle = self.mutators[usize::from(index)].handler;
            Ok(Some(handle(self, event)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_asset(&self, asset: &CheapString) -> Option<Rc<Vec<u8>>> {
        Some(self.assets.get(asset)?.clone())
    }

    pub fn requested(&self) -> Option<CheapString> {
        self.requests.first().map(|r| r.asset.clone())
    }

    /// If `asset` is already loaded, this will trigger
    /// Handling of an `AssetLoaded` event immediately
    pub fn request(&mut self, asset: CheapString, origin: NodeKey) -> Result<(), Error> {
        if self.assets.contains_key(&asset) {
            self.handle(origin, Event::AssetLoaded {
                node_key: origin,
            })?.ok_or_else(|| error!())
        } else {
            self.requests.push(Request {
                asset,
                origin,
            });
            Ok(())
        }
    }

    pub fn data_response(&mut self, asset: CheapString, data: Vec<u8>) -> Result<(), Error> {
        self.assets.insert(asset.clone(), Rc::new(data));

        let mut i = 0;
        while let Some(request) = self.requests.get(i) {
            if request.asset == asset {
                let node_key = self.requests.swap_remove(i).origin;
                self.handle(node_key, Event::AssetLoaded {
                    node_key,
                })?;
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
                    path_hash.write(path_step.as_bytes());
                    current = match current.get_mut(path_step) {
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

    pub fn attr(&mut self, node: NodeKey, attr: &str, default: Option<CheapString>) -> Result<StateFinderResult, Error> {
        let xml_node_index = self.view[node].xml_node_index.get()
            .expect("cannot use Application::attr on nodes without xml_node_index");
        let xml_node_key = self.xml_tree.node_key(xml_node_index);
        let xml_node = &self.xml_tree[xml_node_key];

        let mutator_index = xml_node.factory.get().unwrap();
        let mutator = &self.mutators[usize::from(mutator_index)];
        let tag = mutator.xml_tag.as_ref().unwrap();

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
                None => Err(error!("Missing {:?} attribute on <{}>", attr, tag.deref())),
            }
        }
    }

    fn build_render_list(&mut self, render_list: &mut Vec<(Position, Size)>, fb_rect: &(Position, Size), key: NodeKey) {
        let node = &mut self.view[key];
        if node.layout_config.get_dirty() {
            node.layout_config.set_dirty(false);
            let mut rect = (node.position, node.size);
            constrain(fb_rect, &mut rect);
            render_list.push(rect);
            // all children that are in the container will also be re-rendered
        } else {
            for_each_child!(self.view, key, child, {
                self.build_render_list(render_list, fb_rect, child);
            });
        }
    }

    fn paint(
        &mut self,
        key: NodeKey,
        fb: &mut [RGBA8],
        stride: usize,
        render_list: &[(Position, Size)],
        restrict: &mut (Position, Size),
    ) -> Result<(), Error> {
        let size = self.view[key].size;
        let position = self.view[key].position;
        let texture_coords = (position, size);

        let backup = *restrict;
        constrain(&texture_coords, restrict);

        for sampling_window in render_list {
            let mut sampling_window = *sampling_window;
            constrain(&texture_coords, &mut sampling_window);
            constrain(restrict, &mut sampling_window);
            self.view[key].background.paint(fb, texture_coords, sampling_window, stride, 2, true);
        }

        for_each_child!(self.view, key, child, {
            self.paint(child, fb, stride, render_list, restrict)?;
        });


        for sampling_window in render_list {
            let mut sampling_window = *sampling_window;
            constrain(&texture_coords, &mut sampling_window);
            constrain(restrict, &mut sampling_window);
            // super::visual::debug_framebuffer(fb, stride, sampling_window);

            self.view[key].foreground.paint(fb, texture_coords, sampling_window, stride, 2, true);
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
    ) {
        let stride = fb_size.0;
        let new_size = Size::new(Pixels::from_num(stride), Pixels::from_num(fb_size.1));

        let _mouse = Position::new(SignedPixels::from_num(mx), SignedPixels::from_num(my));

        if _click {
            let mut path_hash = Hasher::default();
            let mut array = self.state_lookup(self.root, "root", "test", &mut path_hash).unwrap().clone();
            if let Some(array) = array.as_array_mut() {
                array.push(StateValue::String(crate::format!("damn {}", array.len())));
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
        let mut render_list = Vec::new();

        if self.view[self.root].size != new_size {
            // log::info!("fbsize: {}x{}", stride, fb_size.1);
            self.view[self.root].size = new_size;
            self.must_check_layout = true;
            framebuffer.fill(transparent);
            render_list.push(fb_rect);
        }

        if self.must_check_layout {
            compute_layout(self, self.root).unwrap();
            self.must_check_layout = false;
        }

        if render_list.len() == 0 {
            self.build_render_list(&mut render_list, &fb_rect, self.root);
            for rect in &render_list {
                write_framebuffer(framebuffer, stride, *rect, transparent);
            }
        }

        if render_list.len() > 0 {
            let mut restrict = fb_rect;
            self.paint(self.root, framebuffer, stride, &render_list, &mut restrict).unwrap();
        }
    }
}
