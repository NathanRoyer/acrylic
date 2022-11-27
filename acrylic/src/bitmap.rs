//! Bitmap, Channels, blit_rgba, aspect_ratio

use crate::app::ScratchBuffer;
use crate::app::Application;
use crate::geometry::aspect_ratio;
use crate::node::Axis::Horizontal;
use crate::node::Axis::Vertical;
use crate::node::RenderCache;
use crate::node::RenderReason;
use crate::node::LayerCaching;
use crate::node::LengthPolicy;
use crate::node::Margin;
use crate::node::Node;
use crate::node::NodeBox;
use crate::node::node_box;
use crate::node::NodePathSlice;
use crate::round;
use crate::Size;
use crate::Spot;

use core::any::Any;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;

use alloc::vec;
use alloc::string::String;
use alloc::vec::Vec;

/// Number of channels
pub type Channels = usize;

/// red, green, blue, alpha
pub const RGBA: Channels = 4;

/// red, green, blue
pub const RGB: Channels = 3;

/// black and white
pub const BW: Channels = 1;

/// General-purpose 2D image node.
#[derive(Clone)]
pub struct Bitmap {
    /// The pixel array; Changing its size
    /// is forbidden.
    pub pixels: Vec<u8>,
    /// A resized cache for faster rendering
    pub cache: Vec<u8>,
    /// The number of channels; must be one of
    /// BW, RGB or RGBA. Rendering is only
    /// supported for RGBA images at the moment.
    pub channels: Channels,
    /// The original size (width and height) of the image
    pub size: Size,
    /// The output spot of the node
    pub spot_size: Size,
    /// Optional margin for the node
    pub margin: Option<Margin>,
    /// aspect ratio of the node (taking
    /// the margin into account)
    pub ratio: f64,
    pub render_cache: RenderCache,
    pub render_reason: RenderReason,
}

impl Debug for Bitmap {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Bitmap")
            .field("channels", &self.channels)
            .field("size", &self.size)
            .field("spot_size", &self.spot_size)
            .field("margin", &self.margin)
            .field("ratio", &self.ratio)
            .finish()
    }
}

impl Bitmap {
    /// Creates a new Bitmap Node. Once created,
    /// all pixels will be transparent, but you can
    /// set their color manually via the `pixels` field.
    pub fn new(size: Size, channels: Channels, margin: Option<Margin>) -> Self {
        Self {
            size,
            channels,
            pixels: vec![0; channels * size.w * size.h],
            cache: Vec::new(),
            spot_size: Size::zero(),
            margin,
            ratio: aspect_ratio_with_m(size, margin),
            render_cache: [None, None],
            render_reason: RenderReason::Resized,
        }
    }
}

impl Node for Bitmap {
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
        _scratch: ScratchBuffer,
    ) -> Result<(), ()> {
        if self.render_reason.is_valid() {
            blit_rgba::<true, 2>(
                &self.pixels,
                self.channels,
                self.size,
                spot,
            );
        }
        Ok(())
    }

    fn render_cache(&mut self) -> Result<&mut RenderCache, ()> {
        Ok(&mut self.render_cache)
    }

    fn layers_to_cache(&self) -> LayerCaching {
        LayerCaching::FOREGROUND
    }

    fn validate_spot_size(&mut self, _: Size) {
        self.render_reason = RenderReason::Resized;
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn margin(&self) -> Option<Margin> {
        self.margin
    }

    fn get_spot_size(&self) -> Size {
        self.spot_size
    }

    fn set_spot_size(&mut self, size: Size) {
        self.spot_size = size;
    }

    fn describe(&self) -> String {
        String::from("Image")
    }

    fn please_clone(&self) -> NodeBox {
        node_box(self.clone())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

pub fn aspect_ratio_with_m(size: Size, margin: Option<Margin>) -> f64 {
    let (add_w, add_h) = match margin {
        Some(m) => (m.total_on(Horizontal), m.total_on(Vertical)),
        None => (0, 0),
    };
    aspect_ratio(size.w + add_w, size.h + add_h)
}

pub fn blit_rgba<
    const OWNED_DST: bool,
    const SUBPX: usize,
>(
    src: &[u8],
    src_channels: Channels,
    src_size: Size,
    dst: &mut Spot,
) {
    let aa_unit = 1.0 / (SUBPX as f32);
    let aa_sq = (SUBPX * SUBPX) as u32;
    let dst_size = match dst.inner_crop(true) {
        Some((_, size)) => size,
        None => Size::zero(),
    };
    if dst_size.w > 0 && src_size.w > 0 {
        let dst_w = (dst_size.w - 1) as f32;
        let src_w = (src_size.w - 1) as f32;
        let ratio = src_w / dst_w;
        dst.for_each_line(true, |y, dst_line| {
            for x in 0..dst_size.w {
                let i = x * RGBA;
                let dst_pixel = dst_line.get_mut(i..(i + RGBA)).unwrap();

                let mut src_pixel_u32 = [0; RGBA];
                let mut y_aa = y as f32;
                for _ in 0..SUBPX {
                    let mut x_aa = x as f32;
                    for _ in 0..SUBPX {
                        let x = round!(x_aa * ratio, f32, usize);
                        let y = round!(y_aa * ratio, f32, usize);
                        let j = (y * src_size.w + x) * src_channels;
                        if let Some(p) = src.get(j..(j + src_channels)) {
                            let rgba = match src_channels {
                                BW => [p[0], p[0], p[0], 255],
                                RGB => [p[0], p[1], p[2], 255],
                                RGBA => [p[0], p[1], p[2], p[3]],
                                _ => panic!("could not blit image with {} channels", src_channels),
                            };
                            for c in 0..RGBA {
                                src_pixel_u32[c] += rgba[c] as u32;
                            }
                        }
                        x_aa += aa_unit;
                    }
                    y_aa += aa_unit;
                }

                let mut src_pixel = [0; RGBA];
                for c in 0..RGBA {
                    src_pixel[c] = (src_pixel_u32[c] / aa_sq) as u8;
                }
                if OWNED_DST {
                    dst_pixel.copy_from_slice(&src_pixel);
                } else {
                    let dst_alpha = dst_pixel[3] as u32;
                    let src_alpha = 255 - dst_alpha;
                    for c in 0..RGBA {
                        let src = src_pixel[c] as u32;
                        let dst = dst_pixel[c] as u32;
                        let sum = src * src_alpha + dst * dst_alpha;
                        dst_pixel[c] = (sum / 255) as u8;
                    }
                }
            }
        });
    }
}
