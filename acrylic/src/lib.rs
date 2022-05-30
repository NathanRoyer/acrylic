#![no_std]

extern crate no_std_compat as std;

use node::NodePath;

use std::sync::Mutex;
use std::sync::MutexGuard;

pub mod app;
pub mod bitmap;
pub mod flexbox;
pub mod geometry;
pub mod node;

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "png")]
pub mod png;

#[cfg(feature = "railway")]
pub mod railway;

/// General-purpose position structure
pub type Point = geometry::Point;

/// General-purpose size structure
pub type Size = geometry::Size;

/// Type representing a rectangle
pub type Spot = geometry::Spot;

/// Non-verbose result
pub type Status = Result<(), ()>;

/// Platforms must publicly expose a `log`
/// function which matches this signature.
///
/// ```rust
/// pub fn log(s: &str) {
///     println!("{}", s);
/// }
/// ```
pub type PlatformLog = &'static dyn Fn(&str);

// todo: allow partial renders
/// Platforms must publicly expose a `blit`
/// function which matches this signature.
///
/// On success, the return value is a tuple containing:
/// 1. the buffer slice
/// 2. the pitch (number of bytes between lines in the spot)
/// 3. false if this buffer is shared between nodes at this spot, true otherwise
///
/// ```rust
/// pub fn blit<'a>(spot: &'a Spot, path: Option<&'a NodePath>) -> Option<(&'a mut [u8], usize, bool)> {
///     let slice = /* maybe allocate memory */;
///     let pitch = /* can be zero for non-shared buffers */;
///     let not_shared = /* true if you allocated memory for that node */;
///     (slice, pitch, not_shared)
/// }
/// ```
pub type PlatformBlit =
    &'static dyn for<'a> Fn(&'a Spot, Option<&'a NodePath>) -> Option<(&'a mut [u8], usize, bool)>;

/// `no_std`-friendly wrapper for `mutex.lock()`
pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> Option<MutexGuard<T>> {
    #[cfg(feature = "std")]
    let result = mutex.lock().ok();
    #[cfg(not(feature = "std"))]
    let result = Some(mutex.lock());
    result
}

/// Transforms an `Option<T>` to a `Result<T, ()>`
/// which is compatible with [`Status`]
pub fn status<T>(option: Option<T>) -> Result<T, ()> {
    option.ok_or(())
}

/// `no_std`-friendly [`std::format`](https://doc.rust-lang.org/std/macro.format.html)
#[macro_export]
macro_rules! format {
	($($arg:tt)*) => {{
		let mut string = String::new();
		core::fmt::write(&mut string, core::format_args!($($arg)*)).unwrap();
		string
	}}
}
