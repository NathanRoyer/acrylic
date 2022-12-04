//! Container

use crate::app::Application;
use crate::app::ScratchBuffer;
use crate::node::EventType;
use crate::node::LengthPolicy;
use crate::node::LayerCaching;
use crate::node::RenderCache;
use crate::node::RenderReason;
use crate::node::NodePathSlice;
use crate::node::please_clone_vec;
use crate::node::node_box;
use crate::node::NodeBox;
use crate::node::Margin;
use crate::node::Event;
use crate::node::Node;
use crate::node::Axis;
use crate::Size;
use crate::Spot;

use log::error;
use log::warn;

use core::any::Any;
use core::fmt::Debug;

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "railway")]
use crate::railway::{arg, LoadedRailwayProgram};

#[cfg(feature = "railway")]
use railway::{Couple, Program};

#[cfg(feature = "railway")]
use lazy_static::lazy_static;

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
#[derive(Debug)]
pub struct Container {
    pub children: Vec<Option<NodeBox>>,
    pub policy: LengthPolicy,
    pub on_click: Option<String>,
    pub spot_size: Size,
    pub axis: Axis,
    pub gap: usize,
    pub margin: Option<usize>,
    /// For rounded-corners
    pub radius: Option<usize>,
    pub focused: bool,
    /// Style override
    pub normal_style: Option<usize>,
    /// Style override when focused
    pub focus_style: Option<usize>,
    /// Initialize to `None`
    #[cfg(feature = "railway")]
    pub style_rwy: Option<LoadedRailwayProgram<4>>,
    pub render_cache: RenderCache,
    pub render_reason: RenderReason,
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
    #[allow(unused)]
    fn tick(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        style: usize,
        scratch: ScratchBuffer,
    ) -> Result<bool, ()> {
        self.render_reason.downgrade();
        let dirty = self.render_reason.is_valid();

        #[cfg(feature = "railway")]
        if dirty && self.radius.is_some() {
            if self.style_rwy.is_none() {
                self.style_rwy = Some(CONTAINER_RWY.clone());
            }

            let rwy = self.style_rwy.as_mut().unwrap();

            let size = self.spot_size;
            let parent_bg = app.theme.styles[style].background;
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

            rwy.compute();
        }

        Ok(dirty)
    }

    fn render_background(
        &mut self,
        app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        spot: &mut Spot,
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if let Some(i) = self.style() {
            spot.fill(app.theme.styles[i].background, true);
        }
        Ok(())
    }

    #[cfg(feature = "railway")]
    fn render_foreground(
        &mut self,
        _app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        spot: &mut Spot,
        scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if self.render_reason.is_valid() && self.style_rwy.is_some() {
            let rwy = self.style_rwy.as_mut().unwrap();
            if let Some((_, size)) = spot.inner_crop(false) {
                if let Some((pixels, pitch)) = spot.get(false) {
                    rwy.render(scratch, pixels, pitch, size)?;
                } else {
                    warn!("couldn't get spot: {:?}", spot);
                }
            }
        }
        Ok(())
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }

    #[cfg(feature = "railway")]
    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::FOREGROUND
    }

    #[cfg(not(feature = "railway"))]
    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::empty()
    }

    fn please_clone(&self) -> NodeBox {
        node_box(Self {
            children: please_clone_vec(&self.children),
            policy: self.policy,
            on_click: self.on_click.clone(),
            spot_size: self.spot_size,
            axis: self.axis,
            gap: self.gap,
            margin: self.margin,
            radius: self.radius,
            focused: self.focused,
            normal_style: self.normal_style,
            focus_style: self.focus_style,
            #[cfg(feature = "railway")]
            style_rwy: self.style_rwy.clone(),
            render_cache: self.render_cache.clone(),
            render_reason: self.render_reason.clone(),
        })
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn margin(&self) -> Option<Margin> {
        self.margin.map(|l| Margin::quad(l))
    }

    fn children(&self) -> &[Option<NodeBox>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Option<NodeBox>] {
        &mut self.children
    }

    fn policy(&self) -> LengthPolicy {
        self.policy
    }

    fn add_node(&mut self, child: NodeBox) -> Result<usize, ()> {
        let index = self.children.len();
        self.children.push(Some(child));
        Ok(index)
    }

    fn replace_node(&mut self, index: usize, child: NodeBox) -> Result<(), ()> {
        if let Some(addr) = self.children.get_mut(index) {
            *addr = Some(child);
            Ok(())
        } else {
            Err(error!("Container::replace_node: No such child :|"))
        }
    }

    fn style_override(&self) -> Option<usize> {
        self.style()
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

    fn validate_spot_size(&mut self, _: Size) {
        self.render_reason = RenderReason::Resized;
    }

    fn set_focused(&mut self, focused: bool) -> bool {
        if self.focus_style.is_some() {
            self.render_reason = RenderReason::Resized;
            self.focused = focused;
            true
        } else {
            false
        }
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
        _: NodePathSlice,
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