//! LoadedRailwayProgram, RailwayLodaer, RailwayNode, xml_load_railway

use crate::app::DataRequest;
use crate::app::Application;
use crate::app::ScratchBuffer;
use crate::format;
use crate::geometry::aspect_ratio;
use crate::node::node_box;
use crate::node::RenderCache;
use crate::node::RenderReason;
use crate::node::LengthPolicy;
use crate::node::Node;
use crate::node::NodePathSlice;
use crate::node::NodeBox;
use crate::Size;
use crate::Spot;
use crate::Status;

#[cfg(feature = "xml")]
use crate::xml::unexpected_attr;
#[cfg(feature = "xml")]
use crate::xml::Attribute;
#[cfg(feature = "xml")]
use crate::xml::TreeParser;

use railway::Couple;
use railway::Program;
use railway::RWY_PXF_RGBA8888;

use core::any::Any;

use alloc::string::String;
use alloc::vec::Vec;

/// Resolves an argument to an address in a railway program.
pub fn arg(program: &Program, name: &str, mandatory: bool) -> Result<usize, Option<String>> {
    match program.argument(name) {
        Some(addr) => Ok(addr as usize),
        None => Err(match mandatory {
            true => Some("Missing {} in railway file".into()),
            false => None,
        }),
    }
}

/// Minimal structure which you can store between
/// railway renderings and which holds everything
/// needed to render.
#[derive(Debug, Clone)]
pub struct LoadedRailwayProgram<const A: usize> {
    pub program: Program,
    pub stack: Vec<Couple>,
    pub mask: Vec<u8>,
    pub addresses: [usize; A],
}

const PXF: u8 = RWY_PXF_RGBA8888;

impl<const A: usize> LoadedRailwayProgram<A> {
    pub fn compute(&mut self) {
        self.program.compute(&mut self.stack);
    }

    /// Renders a railway image to a buffer.
    ///
    /// Alpha blending is not implemented here,
    /// so any transparency in the image will
    /// override transparent pixels in the buffer.
    pub fn render(
        &self,
        scratch: ScratchBuffer,
        dst: &mut [u8],
        pitch: usize,
        size: Size
    ) -> Status {
        let scratch_len = size.w * size.h;
        if scratch.len() < scratch_len {
            scratch.resize(scratch_len, 0);
        }
        self.program.render::<PXF, 3>(
            &self.stack,
            dst,
            scratch.as_mut_slice(),
            size.w,
            size.h,
            pitch
        );
        Ok(())
    }
}

/// [`Node`] implementor which displays a railway image.
#[derive(Debug, Clone)]
pub struct RailwayNode {
    pub(crate) lrp: LoadedRailwayProgram<1>,
    pub(crate) time_arg: Option<usize>,
    pub(crate) ratio: f64,
    pub(crate) spot_size: Size,
    pub(crate) render_cache: RenderCache,
    pub(crate) render_reason: RenderReason,
}

impl RailwayNode {
    /// Parse and create a railway node.
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        let program = match Program::parse(bytes) {
            Ok(p) => p,
            Err(e) => Err(format!("{:?}", e))?,
        };
        let stack = program.create_stack();
        program.valid().ok_or(format!("Invalid railway file"))?;
        let size_arg = arg(&program, "size", true).map_err(|o| o.unwrap())?;
        let time_arg = arg(&program, "time", false).ok();
        let size = stack[size_arg];
        let ratio = aspect_ratio(size.x as usize, size.y as usize);
        Ok(Self {
            lrp: LoadedRailwayProgram {
                program,
                stack,
                mask: Vec::new(),
                addresses: [size_arg],
            },
            time_arg,
            ratio,
            spot_size: Size::zero(),
            render_cache: [None, None],
            render_reason: RenderReason::Resized,
        })
    }
}

impl Node for RailwayNode {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn describe(&self) -> String {
        String::from("Railway file")
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, spot_size: Size) {
        self.spot_size = spot_size;
    }

    fn tick(
        &mut self,
        _app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        _scratch: ScratchBuffer,
    ) -> Result<bool, ()> {
        self.render_reason.downgrade();
        Ok(self.render_reason.is_valid())
    }

    fn render_foreground(
        &mut self,
        _app: &mut Application,
        _path: NodePathSlice,
        _style: usize,
        spot: &mut Spot,
        scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if self.render_reason.is_valid() {
            let _ = self.time_arg;
            if let Some((pixels, pitch)) = spot.get(true) {
                self.lrp.render(scratch, pixels, pitch, self.spot_size)?;
            }
        }
        Ok(())
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }

    fn validate_spot_size(&mut self, _: Size) {
        let size = self.spot_size;
        let couple_size = Couple::new(size.w as f32, size.h as f32);
        self.lrp.stack[self.lrp.addresses[0]] = couple_size;
        self.render_reason = RenderReason::Resized;
    }
}

/// [`Node`] implementor which requests PNG bytes
/// then replaces itself with a [`RailwayNode`] once
/// data has been loaded and parsed.
#[derive(Debug, Clone)]
pub struct RailwayLoader {
    source: String,
}

impl Node for RailwayLoader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn describe(&self) -> String {
        String::from("Loading railway file...")
    }

    fn initialize(&mut self, app: &mut Application, path: NodePathSlice) -> Result<(), String> {
        app.data_requests.push(DataRequest {
            node: path.to_vec(),
            name: self.source.clone(),
            range: None,
        });
        Ok(())
    }

    fn loaded(
        &mut self,
        app: &mut Application,
        path: NodePathSlice,
        _: &str,
        _: usize,
        data: &[u8],
    ) -> Status {
        let railway = match RailwayNode::new(data) {
            Err(s) => {
                app.log(&format!("[rwy] loading error: {}", s));
                return Err(());
            }
            Ok(r) => r,
        };

        app.replace_kidnapped(path, node_box(railway));
        Ok(())
    }
}

/// XML tag for Railway Images.
///
/// Pass this to [`TreeParser::with`].
///
/// Results in a [`RailwayLoader`] node.
///
/// ```xml
/// <rwy src="img/image0.rwy" />
/// ```
///
/// The `src` attribute is mandatory and must point to a railway image asset.
#[cfg(feature = "xml")]
pub fn xml_load_railway(
    _: &mut TreeParser,
    attributes: &[Attribute],
) -> Result<Option<NodeBox>, String> {
    let mut source = Err(String::from("missing src attribute"));

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "src" => source = Ok(value.clone()),
            _ => unexpected_attr(&name)?,
        }
    }

    Ok(Some(node_box(RailwayLoader { source: source? })))
}
