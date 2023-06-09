//! Please follow [the Quickstart guide](https://github.com/NathanRoyer/acrylic/blob/main/README.md#%EF%B8%8F-quickstart), then navigate here as you wish.
//!
//! You will find documentation for built-in tags in [`builtin`].
//!
//! Internal processes are also documented in [`core`].

#![no_std]

extern crate alloc;
extern crate std;

use ::core::fmt;

#[doc(hidden)]
pub use {
    alloc::{string::String, vec::Vec, vec, boxed::Box, rc::Rc, format},
    ahash::AHasher as Hasher,
    litemap::LiteMap,
};

pub mod core;
pub mod builtin;
pub mod utils;

pub use utils::{cheap_string::{CheapString, cheap_string}, hash_map::HashMap};

pub const NOTO_SANS: &'static [u8] = include_bytes!("noto-sans.ttf");

pub const DEFAULT_FONT_NAME: &'static str = "default-font";
pub const DEFAULT_FONT_SIZE: &'static str = "24";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: u32,
    pub file: &'static str,
    pub msg: Option<String>,
}

impl Error {
    pub fn new(line: u32, file: &'static str, msg: Option<String>) -> Self {
        Self { line, file, msg }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.msg {
            Some(msg) => write!(f, "{}, line {}: {}", self.file, self.line, msg),
            None => write!(f, "{}, line {}: unknown error", self.file, self.line),
        }
    }
}

/// Creates an [`Error`] automatically with an optional formatted string
///
/// # Examples
///
/// ```ignore
/// Err(error!())?;
/// Err(error!("We crashed... so sad..."))?;
/// Err(error!("Yo this is absurd: {:?}", an_incorrect_value))?;
/// ```
///
/// Corresponding messages:
///
/// ```text
/// src/my_file.rs, line 51: unknown error
/// src/my_file.rs, line 51: We crashed... so sad...
/// src/my_file.rs, line 51: Yo this is absurd: Unicorn { flying: true, rainbows: true }
/// ```
#[macro_export]
macro_rules! error {
    () => { $crate::Error::new(::core::line!(), ::core::file!(), None) };
    ($($arg:tt)*) => { $crate::Error::new(::core::line!(), ::core::file!(), Some($crate::format!($($arg)*))) };
}
