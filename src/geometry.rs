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
}

pub fn aspect_ratio(w: usize, h: usize) -> f64 {
	(w as f64) / (h as f64)
}
