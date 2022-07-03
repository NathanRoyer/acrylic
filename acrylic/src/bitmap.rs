use crate::app::for_each_line;
use crate::app::Application;
use crate::geometry::aspect_ratio;
use crate::node::Axis::Horizontal;
use crate::node::Axis::Vertical;
use crate::node::LengthPolicy;
use crate::node::Margin;
use crate::node::NeedsRepaint;
use crate::node::Node;
use crate::node::NodePath;
use crate::status;
use crate::BlitPath;
use crate::Point;
use crate::Size;
use crate::Spot;
use crate::Status;

use core::any::Any;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;

use std::prelude::v1::vec;
use std::string::String;
use std::vec::Vec;

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
    pub spot: Spot,
    /// Optional margin for the node
    pub margin: Option<Margin>,
    /// aspect ratio of the node (taking
    /// the margin into account)
    pub ratio: f64,
    /// determines if rendering is necessary
    pub repaint: NeedsRepaint,
}

impl Debug for Bitmap {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Bitmap")
            .field("channels", &self.channels)
            .field("size", &self.size)
            .field("spot", &self.spot)
            .field("margin", &self.margin)
            .field("ratio", &self.ratio)
            .finish()
    }
}

const TRANSPARENT_PIXEL: [u8; 4] = [0; 4];

pub fn aspect_ratio_with_m(size: Size, margin: Option<Margin>) -> f64 {
    let (add_w, add_h) = match margin {
        Some(m) => (m.total_on(Horizontal), m.total_on(Vertical)),
        None => (0, 0),
    };
    aspect_ratio(
        (size.w as isize + add_w) as usize,
        (size.h as isize + add_h) as usize,
    )
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
            spot: (Point::zero(), Size::zero()),
            margin,
            repaint: NeedsRepaint::all(),
            ratio: aspect_ratio_with_m(size, margin),
        }
    }

    fn update_cache(&mut self, spot: Spot, owned: bool) -> Status {
        assert!(self.channels == RGBA);
        let (_, size) = spot;
        let len = size.w * size.h * RGBA;
        if len != 0 && len != self.cache.len() {
            self.cache.resize(len, 0);
            let spot_factor = (size.w - 1) as f64;
            let img_factor = (self.size.w - 1) as f64;
            let ratio = img_factor / spot_factor;
            for y in 0..size.h {
                for x in 0..size.w {
                    let i = (y * size.w + x) * RGBA;
                    let x = round((x as f64) * ratio);
                    let y = round((y as f64) * ratio);
                    let j = (y * self.size.w + x) * RGBA;
                    let src = self.pixels.get(j..(j + RGBA)).unwrap_or(&TRANSPARENT_PIXEL);
                    let dst = self.cache.get_mut(i..(i + RGBA)).unwrap();
                    if owned {
                        dst.copy_from_slice(src);
                    } else {
                        // premultiplied alpha
                        let a = src[3] as u32;
                        for i in 0..3 {
                            dst[i] = ((src[i] as u32 * a) / 255) as u8;
                        }
                        dst[3] = a as u8;
                    }
                }
            }
        }
        Ok(())
    }

    /// Updates the cache and tries to render it at `spot`,
    /// regardless of `self.dirty`.
    ///
    /// This method is called manually by nodes embedding
    /// [`Bitmap`]s, such as [`GlyphNode`](`crate::text::GlyphNode`).
    pub fn render_at(&mut self, app: &mut Application, path: &NodePath, spot: Spot) -> Status {
        let (dst, pitch, owned) = app.blit(&spot, BlitPath::Node(path))?;
        self.update_cache(spot, owned)?;
        let (_, size) = spot;
        let px_width = RGBA * size.w;
        if px_width > 0 {
            let mut src = self.cache.chunks(px_width);
            for_each_line(dst, size, pitch, |_, line_dst| {
                let line_src = src.next().unwrap();
                if owned {
                    line_dst.copy_from_slice(line_src);
                } else {
                    let mut i = px_width as isize - 1;
                    let mut a = 0;
                    while i >= 0 {
                        let j = i as usize;
                        let (dst, src) = (&mut line_dst[j], &(line_src[j] as u32));
                        if (j & 0b11) == 3 {
                            a = (255 - *src) as u32;
                        }
                        *dst = (*src + (((*dst as u32) * a) / 255)) as u8;
                        i -= 1;
                    }
                }
            });
        }
        Ok(())
    }
}

impl Node for Bitmap {
    fn render(
        &mut self,
        app: &mut Application,
        path: &mut NodePath,
        _: usize,
    ) -> Result<usize, ()> {
        if self.repaint.contains(NeedsRepaint::FOREGROUND) {
            let spot = status(self.get_content_spot_at(self.spot))?;
            self.render_at(app, path, spot)?;
            self.repaint.remove(NeedsRepaint::FOREGROUND);
        }
        Ok(0)
    }

    fn policy(&self) -> LengthPolicy {
        LengthPolicy::AspectRatio(self.ratio)
    }

    fn repaint_needed(&mut self, repaint: NeedsRepaint) {
        self.repaint.insert(repaint);
    }

    fn margin(&self) -> Option<Margin> {
        self.margin
    }

    fn get_spot(&self) -> Spot {
        self.spot
    }

    fn set_spot(&mut self, spot: Spot) {
        self.repaint = NeedsRepaint::all();
        self.spot = spot;
    }

    fn describe(&self) -> String {
        String::from("Image")
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(feature = "std")]
#[inline(always)]
fn round(float: f64) -> usize {
    float.round() as usize
}

#[cfg(not(feature = "std"))]
#[inline(always)]
fn round(mut float: f64) -> usize {
    // given float > 0
    let integer = float as usize;
    float -= integer as f64;
    match float > 0.5 {
        true => integer + 1,
        false => integer,
    }
}
