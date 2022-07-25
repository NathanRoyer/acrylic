use crate::node::Axis;
use crate::node::Margin;
use crate::bitmap::RGBA;
use crate::style::Color;

use core::fmt::Debug;
use core::fmt::Formatter;
use core::fmt::Result as FmtResult;

/// General-purpose position structure
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub x: isize,
    pub y: isize,
}

impl Point {
    pub const fn new(x: isize, y: isize) -> Self {
        Self { x, y }
    }

    pub const fn zero() -> Self {
        Self::new(0, 0)
    }

    pub fn add_to_axis(&mut self, axis: Axis, operand: isize) {
        *match axis {
            Axis::Horizontal => &mut self.x,
            Axis::Vertical => &mut self.y,
        } += operand as isize;
    }
}

/// General-purpose size structure
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Size {
    /// Width
    pub w: usize,
    /// Height
    pub h: usize,
}

impl Size {
    pub const fn new(w: usize, h: usize) -> Self {
        Self { w, h }
    }

    pub const fn zero() -> Self {
        Self::new(0, 0)
    }

    pub fn get_for_axis(&self, axis: Axis) -> usize {
        match axis {
            Axis::Horizontal => self.w,
            Axis::Vertical => self.h,
        }
    }
}

// TODO: rename to Spot after OG Spot elimination
pub struct NewSpot<'a> {
    pub window: (Point, Size, Option<Margin>),
    pub framebuffer: &'a mut [u8],
    pub fb_size: Size,
}

impl<'a> NewSpot<'a> {
    pub fn inner_crop(&self, inner: bool) -> Option<(Point, Size)> {
        let (mut top_left, mut size, margin) = self.window;
        if inner {
            if let Some(margin) = margin {
                top_left.x += margin.left as isize;
                top_left.y += margin.top as isize;
                let w = size.w.checked_sub(margin.total_on(Axis::Horizontal))?;
                let h = size.h.checked_sub(margin.total_on(Axis::Vertical))?;
                size = Size::new(w, h);
            }
        }
        Some((top_left, size))
    }

    fn offset_pitch(&self, inner: bool) -> Option<(usize, usize)> {
        let (top_left, size) = self.inner_crop(inner)?;

        let w = size.w as isize;
        let h = size.h as isize;
        let x_max = self.fb_size.w as isize;
        let y_max = self.fb_size.h as isize;
        let nw = top_left;
        let se = Point::new(nw.x + w, nw.y + h);

        if nw.x >= 0 && nw.y >= 0 && se.x <= x_max && se.y <= y_max {
            let pitch = (self.fb_size.w - size.w) * RGBA;
            let offset = RGBA * ((nw.y as usize) * self.fb_size.w + (nw.x as usize));
            Some((offset, pitch))
        } else {
            None
        }
    }

    pub fn get(&mut self, inner: bool) -> Option<(&mut [u8], usize)> {
        let (offset, pitch) = self.offset_pitch(inner)?;
        Some((&mut self.framebuffer[offset..], pitch))
    }

    /// This utility function calls `f` for each line
    /// in a buffer.
    ///
    /// These line will have a length of `size.w` and
    /// there will be `size.h` calls. The first argument
    /// of `f` is the line index, starting from `0`.
    pub fn for_each_line(
        &mut self,
        inner: bool,
        mut f: impl FnMut(usize, &mut [u8]),
    ) {
        if let Some((_, size)) = self.inner_crop(inner) {
            if let Some((pixels, pitch)) = self.get(inner) {
                let px_width = size.w * RGBA;
                let mut start = 0;
                let mut stop = px_width;
                let advance = px_width + pitch;
                for y in 0..size.h {
                    f(y, &mut pixels[start..stop]);
                    start += advance;
                    stop += advance;
                }
            }
        }
    }

    pub fn blit(
        &mut self,
        top_layer: &[u8],
        inner: bool,
    ) {
        let w = match self.inner_crop(inner) {
            Some((_,  size)) => size.w,
            None => 0,
        };
        let mut i = 0;
        self.for_each_line(inner, |_, line| {
            let mut x = 0;
            for _ in 0..w {
                let tl_pixel = &top_layer[(i + x)..][..RGBA];
                let line_pixel = &mut line[x..][..RGBA];
                let tl_alpha = tl_pixel[3] as u32;
                /*__*/ if tl_alpha == 0 {
                    // do nothing
                } else if tl_alpha == 255 {
                    line_pixel.copy_from_slice(tl_pixel);
                } else {
                    for c in 0..RGBA {
                        let new = tl_pixel[c] as u32;
                        let old = line_pixel[c] as u32;
                        let total = new * tl_alpha + old * (255 - tl_alpha);
                        line_pixel[c] = (total / 255) as u8;
                    }
                }
                x += RGBA;
            }
            i += x;
        });
    }

    pub fn set_window(
        &mut self,
        window: (Point, Size, Option<Margin>),
    ) {
        self.window = window;
    }

    pub fn fill(&mut self, color: Color, inner: bool) {
        self.for_each_line(inner, |_, dst_line| {
            for i in 0..dst_line.len() {
                dst_line[i] = color[i % RGBA];
            }
        });
    }
}

impl<'a> Debug for NewSpot<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("NewSpot")
            .field("window", &self.window)
            .field("fb_size", &self.fb_size)
            .field("fb_len", &self.framebuffer.len())
            .finish()
    }
}

/// Utility to compute an aspect-ratio
pub fn aspect_ratio(w: usize, h: usize) -> f64 {
    (w as f64) / (h as f64)
}
