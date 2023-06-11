//! Please follow [the Quickstart guide](https://github.com/NathanRoyer/acrylic/blob/main/README.md#%EF%B8%8F-quickstart), then navigate here as you wish.
//!
//! You will find documentation for built-in tags in [`builtin`].
//!
//! Internal processes are also documented in [`core`].

#![no_std]

extern crate alloc;
extern crate std;

use ::core::{fmt, mem::ManuallyDrop};

#[doc(hidden)]
pub use {
    alloc::{string::String, vec::Vec, vec, boxed::Box, rc::Rc, format},
    lmfu::{
        arcstr::{ArcStr, literal as ro_string},
        hash_map::HashMap, LiteMap, self,
    },
};

pub mod core;
pub mod builtin;

pub const NOTO_SANS: &'static [u8] = include_bytes!("noto-sans.ttf");

pub(crate) const DEFAULT_FONT_NAME: ManuallyDrop<ArcStr> = ManuallyDrop::new(ro_string!("default-font"));
pub(crate) const DEFAULT_FONT_SIZE: ManuallyDrop<ArcStr> = ManuallyDrop::new(ro_string!("24"));

pub(crate) const ZERO_ARCSTR: ManuallyDrop<ArcStr> = ManuallyDrop::new(ro_string!("0"));
pub(crate) const ONE_ARCSTR: ManuallyDrop<ArcStr> = ManuallyDrop::new(ro_string!("1"));

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
