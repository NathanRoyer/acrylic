use crate::node::Axis;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
	pub x: isize,
	pub y: isize,
}

impl Point {
	pub fn new(x: isize, y: isize) -> Self {
		Self {
			x,
			y,
		}
	}

	pub fn zero() -> Self {
		Self::new(0, 0)
	}

	pub fn add_to_axis(&mut self, axis: Axis, operand: isize) {
		*match axis {
			Axis::Horizontal => &mut self.x,
			Axis::Vertical   => &mut self.y,
		} += operand as isize;
	}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Size {
	pub w: usize,
	pub h: usize,
}

impl Size {
	pub fn new(w: usize, h: usize) -> Self {
		Self {
			w,
			h,
		}
	}

	pub fn zero() -> Self {
		Self::new(0, 0)
	}

	pub fn get_for_axis(&self, axis: Axis) -> usize {
		match axis {
			Axis::Horizontal => self.w,
			Axis::Vertical   => self.h,
		}
	}
}

pub type Spot = (Point, Size);

pub fn aspect_ratio(w: usize, h: usize) -> f64 {
	(w as f64) / (h as f64)
}
