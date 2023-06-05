//! Rendering, SSAA, Positioning Utilities

use static_assertions::const_assert_eq;
use fixed::types::{U20F12, U12F20, I21F11};
use fixed::traits::LosslessTryFrom;
use super::rgb::{RGBA, RGBA8, RGB8, FromSlice, alt::Gray};
use crate::{Box, Vec, Rc};
use core::fmt::Debug;

pub type Pixels = U20F12;
pub type SignedPixels = I21F11;
pub type Ratio = U12F20;

/// Possible ways for a node to be positioned
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Node will be ignored by the layout algorithm
    #[default]
    Unset,
    // needs two passes in diff-axis config
    // needs one pass in same-axis config
    /// Main length is just enough to contain all children.
    /// Valid for containers only.
    WrapContent,
    /// Main length is a fixed number of pixels.
    Fixed(Pixels),
    /// Main length is divided in chunks of specified
    /// length (in pixels). The number of chunks is
    /// determined by the contained nodes: there will
    /// be as many chunks as necessary for all children
    /// to fit in.
    /// For this to work, the node must be:
    /// * A vertical container in an vorizontal container, or
    /// * An horizontal container in a vertical container.
    Chunks(Pixels),
    /// Main length is computed from the cross length
    /// so that the size of the node maintains a certain
    /// aspect ratio.
    AspectRatio(Ratio),
    /// After neighbors with a different policy are layed
    /// out, nodes with this policy are layed-out so that
    /// they occupy the remaining space in their container.
    /// The `Ratio` is the relative "weight" of this node:
    /// heavier nodes will get more space, lighter nodes
    /// will get less space. If they all have the same
    /// weight, they will all get the same space.
    Remaining(Ratio),
}

/// Utility to compute an aspect-ratio
pub fn aspect_ratio(width: usize, height: usize) -> Ratio {
    if width != 0 && height != 0 {
        Ratio::from_num(width) / Ratio::from_num(height)
    } else {
        Ratio::ZERO
    }
}

/// A structure storing a [`LayoutMode`], an [`Axis`] and a [`Pixels`]
/// struct (representing a container gap) in 8 bytes.
#[derive(Debug, Copy, Clone, Default)]
pub struct LayoutConfig {
    cfg: u32,
    arg: f32,
}

const AXIS_SHIFT: usize = 31;
const MODE_SHIFT: usize = 28;
const DIRT_SHIFT: usize = 27;
const SZFD_SHIFT: usize = 26;
const RESZ_SHIFT: usize = 25;
const AXIS_MASK: u32 = 0x80_00_00_00;
const MODE_MASK: u32 = 0x70_00_00_00;
const DIRT_MASK: u32 = 0x08_00_00_00;
const SZFD_MASK: u32 = 0x04_00_00_00;
const RESZ_MASK: u32 = 0x02_00_00_00;
const  GAP_MASK: u32 = 0x01_ff_ff_ff;

impl LayoutConfig {
    #[inline(always)]
    pub fn set_content_axis(&mut self, content_axis: Axis) {
        self.cfg &= !AXIS_MASK;
        self.cfg |= match content_axis {
            Axis::Horizontal => 0 << AXIS_SHIFT,
            Axis::Vertical   => 1 << AXIS_SHIFT,
        };
    }

    #[inline(always)]
    pub const fn get_content_axis(&self) -> Axis {
        match (self.cfg & AXIS_MASK) >> AXIS_SHIFT {
            0 => Axis::Horizontal,
            _ => Axis::Vertical,
        }
    }

    #[inline(always)]
    pub fn get_length_axis(&self) -> Axis {
        let axis = self.get_content_axis();
        match self.get_layout_mode() {
            LayoutMode::Chunks(_) => axis.complement(),
            _ => axis,
        }
    }

    #[inline(always)]
    pub fn set_dirty(&mut self, dirty: bool) {
        self.cfg &= !DIRT_MASK;
        self.cfg |= match dirty {
            false => 0 << DIRT_SHIFT,
            true  => 1 << DIRT_SHIFT,
        };
    }

    #[inline(always)]
    pub const fn get_dirty(&self) -> bool {
        match self.cfg & DIRT_MASK {
            0 => false,
            _ => true,
        }
    }

    #[inline(always)]
    pub fn set_size_found(&mut self, size_found: bool) {
        self.cfg &= !SZFD_MASK;
        self.cfg |= match size_found {
            false => 0 << SZFD_SHIFT,
            true  => 1 << SZFD_SHIFT,
        };
    }

    #[inline(always)]
    pub const fn get_size_found(&self) -> bool {
        match self.cfg & SZFD_MASK {
            0 => false,
            _ => true,
        }
    }

    #[inline(always)]
    pub fn set_resized(&mut self, resized: bool) {
        self.cfg &= !RESZ_MASK;
        self.cfg |= match resized {
            false => 0 << RESZ_SHIFT,
            true  => 1 << RESZ_SHIFT,
        };
    }

    #[inline(always)]
    pub const fn get_resized(&self) -> bool {
        match self.cfg & RESZ_MASK {
            0 => false,
            _ => true,
        }
    }

    #[inline(always)]
    pub const fn get_content_gap(&self) -> Pixels {
        Pixels::from_bits(self.cfg & GAP_MASK)
    }

    #[inline(always)]
    pub fn set_content_gap(&mut self, content_gap: Pixels) {
        let content_gap = content_gap.to_bits();
        assert_eq!(content_gap >> MODE_SHIFT, 0);
        self.cfg &= !GAP_MASK;
        self.cfg |= content_gap;
    }

    #[inline(always)]
    pub fn get_layout_mode(&self) -> LayoutMode {
        match (self.cfg & MODE_MASK) >> MODE_SHIFT {
            0 => LayoutMode::Unset,
            1 => LayoutMode::WrapContent,
            2 => LayoutMode::Fixed(Pixels::from_num(self.arg)),
            3 => LayoutMode::Chunks(Pixels::from_num(self.arg)),
            4 => LayoutMode::AspectRatio(Ratio::from_num(self.arg)),
            5 => LayoutMode::Remaining(Ratio::from_num(self.arg)),
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    pub fn set_layout_mode(&mut self, layout_mode: LayoutMode) {
        let (mode_encoded, arg) = match layout_mode {
            LayoutMode::Unset              => (0 << MODE_SHIFT, 0.0),
            LayoutMode::WrapContent        => (1 << MODE_SHIFT, 0.0),
            LayoutMode::Fixed(pixels)      => (2 << MODE_SHIFT, pixels.to_num()),
            LayoutMode::Chunks(pixels)     => (3 << MODE_SHIFT, pixels.to_num()),
            LayoutMode::AspectRatio(ratio) => (4 << MODE_SHIFT, ratio.to_num()),
            LayoutMode::Remaining(weight)  => (5 << MODE_SHIFT, weight.to_num()),
        };

        self.cfg &= !MODE_MASK;
        self.cfg |= mode_encoded;
        self.arg = arg;
    }
}

#[test]
fn layout_config() {
    let px = Pixels::from_num(3.5);
    let ratio = Ratio::from_num(3.5);
    let axis = Axis::Vertical;
    let layout_mode = LayoutMode::Remaining(ratio);
    let cfg = LayoutConfig::new_container(axis, px, layout_mode);

    assert_eq!(cfg.content_axis(), axis);
    assert_eq!(cfg.content_gap(), px);
    assert_eq!(cfg.layout_mode(), layout_mode);
}

/// General-purpose axis enumeration
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum Axis {
    #[default]
    Horizontal,
    Vertical,
}

impl Axis {
    #[inline(always)]
    pub fn is(self, other: Self) -> Option<()> {
        if other == self {
            Some(())
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn complement(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

/// General-purpose position structure
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub x: SignedPixels,
    pub y: SignedPixels,
}

impl Position {
    #[inline(always)]
    pub const fn new(x: SignedPixels, y: SignedPixels) -> Self {
        Self { x, y }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(SignedPixels::ZERO, SignedPixels::ZERO)
    }

    #[inline(always)]
    pub fn add_to_axis(&mut self, axis: Axis, operand: SignedPixels) {
        *match axis {
            Axis::Horizontal => &mut self.x,
            Axis::Vertical => &mut self.y,
        } += operand;
    }

    #[inline(always)]
    pub fn add_size(mut self, size: Size) -> Self {
        self.x += size.w.to_num::<SignedPixels>();
        self.y += size.h.to_num::<SignedPixels>();
        self
    }

    #[inline(always)]
    pub fn get_for_axis(&self, axis: Axis) -> SignedPixels {
        match axis {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }
}

/// General-purpose size structure
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Size {
    /// Width
    pub w: Pixels,
    /// Height
    pub h: Pixels,
}

impl Size {
    #[inline(always)]
    pub const fn new(w: Pixels, h: Pixels) -> Self {
        Self { w, h }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(Pixels::ZERO, Pixels::ZERO)
    }

    #[inline(always)]
    pub fn is_zero(self) -> bool {
        self.w == Pixels::ZERO || self.h == Pixels::ZERO
    }

    #[inline(always)]
    pub fn get_for_axis(&self, axis: Axis) -> Pixels {
        match axis {
            Axis::Horizontal => self.w,
            Axis::Vertical => self.h,
        }
    }
}

/// This can be used by nodes to offset the boundaries of
/// their original rendering window.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Margin {
    pub top_left: Size,
    pub bottom_right: Size,
}

impl Margin {
    #[inline(always)]
    pub fn new(top: Pixels, bottom: Pixels, left: Pixels, right: Pixels) -> Self {
        Self {
            top_left: Size::new(top, left),
            bottom_right: Size::new(right, bottom),
        }
    }

    #[inline(always)]
    pub fn quad(value: Pixels) -> Self {
        Self::new(value, value, value, value)
    }

    #[inline(always)]
    pub fn total_on(&self, axis: Axis) -> Pixels {
        self.top_left.get_for_axis(axis) + self.bottom_right.get_for_axis(axis)
    }

    #[inline(always)]
    pub fn is_non_zero(&self) -> bool {
        *self != Margin::quad(Pixels::ZERO)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub trait AsRgba: Copy + Into<RGBA8> + Debug {
    fn has_alpha() -> bool;
}

impl AsRgba for RGBA8 { fn has_alpha() -> bool { true } }
impl AsRgba for RGB8 { fn has_alpha() -> bool { false } }

const_assert_eq!(core::mem::size_of::<RGBA8>(), 4);
const_assert_eq!(core::mem::size_of::<RGB8>(), 3);

/// Blend two colors together
#[inline(always)]
pub fn blend_pixel(src_pixel: RGBA8, dst_pixel: &mut RGBA8) {
    let src_alpha = src_pixel.a as u32;
    let u8_max = u8::MAX as u32;
    let dst_alpha = u8_max - src_alpha;

    let blend = |src, dst: &mut _| {
        if src_alpha == 255 {
            *dst = src;
        } else if src_alpha != 0 {
            let src_scaled = (src as u32) * src_alpha;
            let dst_scaled = (*dst as u32) * dst_alpha;
            *dst = ((src_scaled + dst_scaled) / u8_max) as u8;
        }
    };

    blend(src_pixel.r, &mut dst_pixel.r);
    blend(src_pixel.g, &mut dst_pixel.g);
    blend(src_pixel.b, &mut dst_pixel.b);
    blend(src_pixel.a, &mut dst_pixel.a);
}

/// Trait for anything that can be painted onto the framebuffer
pub trait Texture: Debug {
    fn paint(
        &self,
        framebuffer: &mut [RGBA8],
        texture_coords: (Position, Size),
        sampling_window: (Position, Size),
        dst_stride: usize,
        ssaa: usize,
        alpha_blend: bool,
    );
}

/// Core Texture Types
#[derive(Debug)]
pub enum PixelSource {
    Texture(Box<dyn Texture>),
    RcTexture(Rc<dyn Texture>),
    TextureNoSSAA(Box<dyn Texture>),
    SolidColor(RGBA8),
    Debug,
    None,
}

impl Texture for PixelSource {
    fn paint(
        &self,
        framebuffer: &mut [RGBA8],
        texture_coords: (Position, Size),
        sampling_window: (Position, Size),
        dst_stride: usize,
        ssaa: usize,
        alpha_blend: bool,
    ) {
        if texture_coords.1.is_zero() || sampling_window.1.is_zero() {
            return;
        }

        match self {
            PixelSource::Texture(texture) => {
                texture.paint(framebuffer, texture_coords, sampling_window, dst_stride, ssaa, alpha_blend);
            },
            PixelSource::RcTexture(texture) => {
                texture.paint(framebuffer, texture_coords, sampling_window, dst_stride, ssaa, alpha_blend);
            },
            PixelSource::TextureNoSSAA(texture) => {
                texture.paint(framebuffer, texture_coords, sampling_window, dst_stride, 1, alpha_blend);
            },
            PixelSource::Debug => {
                let x = texture_coords.0.x.to_num::<isize>();
                let y = texture_coords.0.y.to_num::<isize>();
                let width  = texture_coords.1.w.to_num::<isize>();
                let mut height = texture_coords.1.h.to_num::<isize>();

                let mut set_red = |x, y| {
                    if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
                        if x < dst_stride {
                            let offset = y * dst_stride + x;
                            if let Some(slot) = framebuffer.get_mut(offset) {
                                *slot = RGBA8::new(255, 0, 0, 255);
                            }
                        }
                    }
                };

                for x in x..(x + width) {
                    set_red(x, y);
                }

                // sub two lines
                height = height.checked_sub(1).unwrap_or(0);
                let last_line = height;
                height = height.checked_sub(1).unwrap_or(0);
                let last = width.checked_sub(1).unwrap_or(0);

                for y in y..(y + height) {
                    set_red(x, y + 1);
                    set_red(x + last, y + 1);
                }

                for x in x..(x + width) {
                    set_red(x, y + last_line);
                }
            },
            PixelSource::SolidColor(color) => {
                let width  = texture_coords.1.w.to_num();
                let height = texture_coords.1.h.to_num();
                let fpb = FakePixelBuffer::new_fake(*color, width, height);
                fpb.paint(framebuffer, texture_coords, sampling_window, dst_stride, ssaa, alpha_blend);
            },
            PixelSource::None => (),
        }
    }
}

impl Default for PixelSource {
    fn default() -> Self {
        Self::None
    }
}

/// Trait for 2D-sized & indexed pixel storage
pub trait PixelBuffer {
    fn buffer(&self, index: usize) -> RGBA8;
    fn width (&self) -> usize;
    fn height(&self) -> usize;
    fn new(buffer: Box<[u8]>, width: usize, height: usize) -> Self;
    fn has_alpha() -> bool;
}

#[derive(Debug)]
struct FakePixelBuffer {
    color: RGBA8,
    width: usize,
    height: usize,
}

impl PixelBuffer for FakePixelBuffer {
    fn buffer(&self, _index: usize) -> RGBA8 { self.color }
    fn width (&self) -> usize { self.width }
    fn height(&self) -> usize { self.height }
    fn new(_buffer: Box<[u8]>, _width: usize, _height: usize) -> Self { unreachable!() }
    fn has_alpha() -> bool { true }
}

impl FakePixelBuffer {
    pub fn new_fake(color: RGBA8, width: usize, height: usize) -> Self {
        Self {
            color,
            width: width,
            height: height,
        }
    }
}

macro_rules! pixel_buffer {
    ($name:ident, $type:ty, $method:ident, $to_rgba:ident, $has_alpha:expr) => {
        #[derive(Clone)]
        pub struct $name {
            buffer: Box<[u8]>,
            width: usize,
            height: usize,
        }

        impl PixelBuffer for $name {
            fn buffer(&self, index: usize) -> RGBA8 {
                use core::ops::Deref;
                $to_rgba(self.buffer.deref().$method()[index])
            }

            fn width (&self) -> usize { self.width }
            fn height(&self) -> usize { self.height }
            fn has_alpha() -> bool { $has_alpha }

            fn new(buffer: Box<[u8]>, width: usize, height: usize) -> Self {
                assert_eq!(buffer.len(), width * height * ::core::mem::size_of::<$type>());
                Self {
                    buffer,
                    width,
                    height,
                }
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(stringify!($name))
                 .field("width", &self.width)
                 .field("height", &self.height)
                 .finish()
            }
        }
    };
}

fn as_rgba_to_rgba<T: AsRgba>(this: T) -> RGBA8 { this.into() }
fn gray_to_rgba(this: Gray<u8>) -> RGBA8 { RGBA8::new(255, 255, 255, *this) }

pixel_buffer!(GrayScalePixelBuffer, Gray<u8>, as_gray, gray_to_rgba, true);
pixel_buffer!(RgbPixelBuffer, RGB8, as_rgb, as_rgba_to_rgba, false);
pixel_buffer!(RgbaPixelBuffer, RGBA8, as_rgba, as_rgba_to_rgba, true);

/// Paint a rectangle of a framebuffer with a solid color
pub fn write_framebuffer(fb: &mut [RGBA8], stride: usize, window: (Position, Size), color: RGBA8) {
    let src = PixelSource::SolidColor(color);
    src.paint(fb, window, window, stride, 1, false);
}

/// Highlight a rectangle in a framebuffer
pub fn debug_framebuffer(fb: &mut [RGBA8], stride: usize, window: (Position, Size)) {
    let src = PixelSource::Debug;
    src.paint(fb, window, window, stride, 1, false);
}

impl<T> Texture for T where T: Debug + PixelBuffer {
    fn paint(
        &self,
        framebuffer: &mut [RGBA8],
        texture_coords: (Position, Size),
        sampling_window: (Position, Size),
        dst_stride: usize,
        ssaa: usize,
        alpha_blend: bool,
    ) {
        let texture_size = Size::new(
            Pixels::from_num(self.width()),
            Pixels::from_num(self.height()),
        );

        if texture_coords.1.is_zero() || sampling_window.1.is_zero() || texture_size.is_zero() {
            return;
        }

        let x_offset = sampling_window.0.x - texture_coords.0.x;
        let y_offset = sampling_window.0.y - texture_coords.0.y;

        let x_min: usize = sampling_window.0.x.to_num();
        let x_max = x_min + sampling_window.1.w.to_num::<usize>();

        let y_min: usize = sampling_window.0.y.to_num();
        let y_max = y_min + sampling_window.1.h.to_num::<usize>();

        let ratio = (texture_size.h / texture_coords.1.h).to_num::<SignedPixels>();

        let ssaa_unit = ratio / SignedPixels::from_num(ssaa);
        let ssaa_init = SignedPixels::ZERO; // ssaa_unit / 2.0;
        let mut line = y_min * dst_stride;
        let mut samp_y = y_offset * ratio;
        let samp_x_init = x_offset * ratio;

        for _ in y_min..y_max {
            let mut samp_x = samp_x_init;
            for x in x_min..x_max {
                let dst_pixel = &mut framebuffer[line + x];

                let mut src_pixel_u32: RGBA<u32> = Default::default();
                let mut ssaa_px = 0;

                let mut ssaa_y = SignedPixels::ZERO;
                for _ in 0..ssaa {
                    let mut ssaa_x = SignedPixels::ZERO;
                    for _ in 0..ssaa {
                        let texture_x: usize = (samp_x + ssaa_init + ssaa_x).round().to_num();
                        let texture_y: usize = (samp_y + ssaa_init + ssaa_y).round().to_num();

                        if texture_x < self.width() && texture_y < self.height() {
                            let p = self.buffer(texture_y * self.width() + texture_x);
                            src_pixel_u32 += RGBA::<u32>::new(p.r as _, p.g as _, p.b as _, p.a as _);
                            ssaa_px += 1;
                        }

                        ssaa_x += ssaa_unit;
                    }

                    ssaa_y += ssaa_unit;
                }

                if ssaa_px != 0 {
                    let p = src_pixel_u32 / ssaa_px;
                    let src_pixel = RGBA8::new(p.r as _, p.g as _, p.b as _, p.a as _);

                    if alpha_blend {
                        blend_pixel(src_pixel, dst_pixel);
                    } else {
                        *dst_pixel = src_pixel;
                    }
                }

                samp_x += ratio;
            }

            line += dst_stride;
            samp_y += ratio;
        }
    }
}

type Pushes = tinyvec::ArrayVec<[(Position, Size); 4]>;

pub fn push_render_zone(render_list: &mut Vec<(Position, Size)>, push: (Position, Size)) {
    let mut todo = [(push, 0)].to_vec();
    let initial_len = render_list.len();
    while let Some((mut candidate, start)) = todo.pop() {
        for i in start..initial_len {
            if candidate.1.is_zero() {
                break;
            }

            let pushes = split_on_overlap(&mut render_list[i], &candidate);
            match pushes.len() {
                0 => break,
                1 => candidate = pushes[0],
                _ => {
                    candidate = pushes[0];
                    for push in &pushes[1..] {
                        todo.push((*push, i + 1));
                    }
                },
            }
        }

        if !candidate.1.is_zero() {
            render_list.push(candidate);
        }
    }
}

fn split_on_overlap(
    rect_0: &mut (Position, Size),
    rect_1: &(Position, Size),
) -> Pushes {
    type SP = SignedPixels;

    let rect = |x_min: SP, x_max: SP, y_min: SP, y_max: SP| -> (Position, Size) {
        let width  = (x_max - x_min).to_num();
        let height = (y_max - y_min).to_num();
        (Position::new(x_min, y_min), Size::new(width, height))
    };

    let mut pushes = Pushes::new();

    let x_min_0 = rect_0.0.x;
    let y_min_0 = rect_0.0.y;
    let x_max_0 = x_min_0 + rect_0.1.w.to_num::<SignedPixels>();
    let y_max_0 = y_min_0 + rect_0.1.h.to_num::<SignedPixels>();

    let x_min_1 = rect_1.0.x;
    let y_min_1 = rect_1.0.y;
    let x_max_1 = x_min_1 + rect_1.1.w.to_num::<SignedPixels>();
    let y_max_1 = y_min_1 + rect_1.1.h.to_num::<SignedPixels>();

    let x_min_0_in = x_min_0 >= x_min_1 && x_min_0 <= x_max_1;
    let x_max_0_in = x_max_0 >= x_min_1 && x_max_0 <= x_max_1;
    let x_min_1_in = x_min_1 >= x_min_0 && x_min_1 <= x_max_0;
    let x_max_1_in = x_max_1 >= x_min_0 && x_max_1 <= x_max_0;
    let x_overlap = x_min_0_in || x_max_0_in || x_min_1_in || x_max_1_in;

    let y_min_0_in = y_min_0 >= y_min_1 && y_min_0 <= y_max_1;
    let y_max_0_in = y_max_0 >= y_min_1 && y_max_0 <= y_max_1;
    let y_min_1_in = y_min_1 >= y_min_0 && y_min_1 <= y_max_0;
    let y_max_1_in = y_max_1 >= y_min_0 && y_max_1 <= y_max_0;
    let y_overlap = y_min_0_in || y_max_0_in || y_min_1_in || y_max_1_in;

    if x_min_0_in && x_max_0_in && y_min_0_in && y_max_0_in {
        // rect_0 is contained in rect_1
        *rect_0 = *rect_1;
    } else if x_overlap && y_overlap {
        let middle_y_min;
        let middle_y_max;

        // the part above
        if y_min_0_in {
            middle_y_min = y_min_0;
            pushes.push(rect(x_min_1, x_max_1, y_min_1, y_min_0));
        } else {
            middle_y_min = y_min_1;
        }

        // the part below
        if y_max_0_in {
            middle_y_max = y_max_0;
            pushes.push(rect(x_min_1, x_max_1, y_max_0, y_max_1));
        } else {
            middle_y_max = y_max_1;
        }

        // the left part
        if x_min_0_in {
            pushes.push(rect(x_min_1, x_min_0, middle_y_min, middle_y_max));
        }

        // the right part
        if x_max_0_in {
            pushes.push(rect(x_max_0, x_max_1, middle_y_min, middle_y_max));
        }
    } else {
        // no overlap between the two rects
        pushes.push(*rect_1);
    }

    pushes
}

/// Resizes a rectangle so that it fits in another one, if it's bigger
#[inline(always)]
pub fn constrain(limits: &(Position, Size), constrained: &mut (Position, Size)) {
    let br_limits = limits.0.add_size(limits.1);

    let x_min_underflow = limits.0.x - constrained.0.x;
    let y_min_underflow = limits.0.y - constrained.0.y;

    if let Some(x_min_underflow) = Pixels::lossless_try_from(x_min_underflow) {
        constrained.0.x = limits.0.x;
        constrained.1.w = constrained.1.w.checked_sub(x_min_underflow).unwrap_or(Pixels::ZERO);
    }

    if let Some(y_min_underflow) = Pixels::lossless_try_from(y_min_underflow) {
        constrained.0.y = limits.0.y;
        constrained.1.h = constrained.1.h.checked_sub(y_min_underflow).unwrap_or(Pixels::ZERO);
    }

    if constrained.0.x > br_limits.x {
        constrained.0.x = br_limits.x;
        constrained.1.w = Pixels::ZERO;
    }

    if constrained.0.y > br_limits.y {
        constrained.0.y = br_limits.y;
        constrained.1.h = Pixels::ZERO;
    }

    let br_constrained = constrained.0.add_size(constrained.1);

    let x_overflow = br_constrained.x - br_limits.x;
    let y_overflow = br_constrained.y - br_limits.y;

    if let Some(x_overflow) = Pixels::lossless_try_from(x_overflow) {
        constrained.1.w = constrained.1.w.checked_sub(x_overflow).unwrap_or(Pixels::ZERO);
    }

    if let Some(y_overflow) = Pixels::lossless_try_from(y_overflow) {
        constrained.1.h = constrained.1.h.checked_sub(y_overflow).unwrap_or(Pixels::ZERO);
    }
}
