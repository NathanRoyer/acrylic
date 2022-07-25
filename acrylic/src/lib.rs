#![no_std]

extern crate alloc;

pub mod app;
pub mod bitmap;
pub mod flexbox;
pub mod geometry;
pub mod node;
pub mod container;
pub mod style;

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "text")]
pub mod font;

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

pub type NewSpot<'a> = geometry::NewSpot<'a>;

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
pub type PlatformLog = fn(&str);

/// Legacy function, just wraps the parameter in `Some`
pub fn lock<T>(mutex: T) -> Option<T> {
    Some(mutex)
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
        let mut string = alloc::string::String::new();
        core::fmt::write(&mut string, core::format_args!($($arg)*)).unwrap();
        string
    }}
}

#[macro_export]
macro_rules! round {
    ($arg:expr, $in:ty, $out:ty) => {{
        // given float > 0
        let mut float = $arg;
        let integer = float as $out;
        float -= integer as $in;
        match float > 0.5 {
            true => integer + 1,
            false => integer,
        }
    }}
}
