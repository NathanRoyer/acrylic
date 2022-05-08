#![no_std]

extern crate no_std_compat as std;

pub mod tree;
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

pub type Void = Option<()>;
