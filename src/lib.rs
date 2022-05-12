#![no_std]

extern crate no_std_compat as std;

pub mod node;
pub mod flexbox;
pub mod geometry;
pub mod bitmap;
pub mod app;

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "png")]
pub mod png;

#[cfg(feature = "railway")]
pub mod railway;

pub type Point = geometry::Point;
pub type Size = geometry::Size;
pub type Spot = geometry::Spot;

pub type Void = Option<()>;

use std::sync::Mutex;
use std::sync::MutexGuard;

pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> Option<MutexGuard<T>> {
	#[cfg(feature = "std")]
	let result = mutex.lock().ok();
	#[cfg(not(feature = "std"))]
	let result = Some(mutex.lock());
	result
}

#[macro_export]
macro_rules! format {
	($($arg:tt)*) => {{
		let mut string = String::new();
		core::fmt::write(&mut string, core::format_args!($($arg)*)).unwrap();
		string
	}}
}
