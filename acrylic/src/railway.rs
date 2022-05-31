use crate::app::Application;
use crate::format;
use crate::geometry::aspect_ratio;
use crate::node::rc_node;
use crate::node::LengthPolicy;
use crate::node::Node;
use crate::node::NodePath;
use crate::node::RcNode;
use crate::BlitPath;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

#[cfg(feature = "xml")]
use crate::app::DataRequest;
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

use std::string::String;
use std::vec::Vec;

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

/// [`Node`] implementor which displays a railway image.
#[derive(Debug, Clone)]
pub struct RailwayNode {
    pub(crate) lrp: LoadedRailwayProgram<1>,
    pub(crate) time_arg: Option<usize>,
    pub(crate) ratio: f64,
    pub(crate) spot: Spot,
}

const PXF: u8 = RWY_PXF_RGBA8888;

impl<const A: usize> LoadedRailwayProgram<A> {
    /// Renders a railway image to a buffer.
    ///
    /// Alpha blending is not implemented here,
    /// so any transparency in the image will
    /// override transparent pixels in the buffer.
    pub fn render(&mut self, dst: &mut [u8], pitch: usize, size: Size) -> Status {
        self.mask.resize(size.w * size.h, 0);
        self.program.compute(&mut self.stack);
        let stack = &self.stack;
        let mask = &mut self.mask;
        self.program
            .render::<PXF>(stack, dst, mask, size.w, size.h, pitch);
        Ok(())
    }
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
            spot: (Point::zero(), Size::zero()),
        })
    }
}

impl Node for RailwayNode {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn describe(&self) -> String {
        String::from("Railway file")
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.spot = spot;
    }

    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        _: usize,
    ) -> Result<usize, ()> {
        let (dst, pitch, _) = app.blit(&self.spot, BlitPath::Node(path))?;
        let (_, size) = self.spot;
        let _ = self.time_arg;
        let c_size = Couple::new(size.w as f32, size.h as f32);
        self.lrp.stack[self.lrp.addresses[0]] = c_size;
        self.lrp.render(dst, pitch, size)?;
        Ok(0)
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

    fn describe(&self) -> String {
        String::from("Loading railway file...")
    }

    fn initialize(&mut self, app: &mut Application, path: &NodePath) -> Result<(), String> {
        app.data_requests.push(DataRequest {
            node: path.clone(),
            name: self.source.clone(),
            range: None,
        });
        Ok(())
    }

    fn loaded(
        &mut self,
        app: &mut Application,
        path: &NodePath,
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
        app.replace_node(path, rc_node(railway)).unwrap();
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
) -> Result<Option<RcNode>, String> {
    let mut source = Err(String::from("missing src attribute"));

    for Attribute { name, value } in attributes {
        match name.as_str() {
            "src" => source = Ok(value.clone()),
            _ => unexpected_attr(&name)?,
        }
    }

    Ok(Some(rc_node(RailwayLoader { source: source? })))
}
