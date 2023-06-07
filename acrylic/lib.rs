//! Please follow [the Quickstart guide](https://github.com/NathanRoyer/acrylic/blob/main/README.md#%EF%B8%8F-quickstart), then navigate here as you wish.
//!
//! You will find documentation for built-in tags in [`builtin`].
//!
//! Internal processes are also documented in [`core`].

#![no_std]

extern crate alloc;
extern crate std;

#[doc(hidden)]
pub use {
    alloc::{string::String, vec::Vec, boxed::Box, rc::Rc, format},
    hashbrown::HashMap,
};

pub mod core;
pub mod builtin;

use ::core::{fmt, str::Split, ops::Deref, hash::{BuildHasher, Hash}};

pub type Hasher = <hashbrown::hash_map::DefaultHashBuilder as BuildHasher>::Hasher;

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

#[derive(Clone, Debug)]
pub enum CheapString {
    String(Rc<String>),
    Static(&'static str),
}

impl Hash for CheapString {
    fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl PartialEq for CheapString {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl Eq for CheapString {}

impl Deref for CheapString {
    type Target = str;

    fn deref(&self) -> &str {
        match self {
            Self::String(s) => &***s,
            Self::Static(s) => s,
        }
    }
}

impl CheapString {
    pub fn split_space(&self) -> Split<char> {
        self.deref().split(' ')
    }
}

impl From<Rc<String>> for CheapString {
    fn from(string: Rc<String>) -> Self {
        CheapString::String(string)
    }
}

impl From<String> for CheapString {
    fn from(string: String) -> Self {
        CheapString::String(Rc::new(string))
    }
}

impl From<&'static str> for CheapString {
    fn from(string: &'static str) -> Self {
        CheapString::Static(string)
    }
}

pub const fn cheap_string(t: &'static str) -> CheapString {
    CheapString::Static(t)
}

impl fmt::Display for CheapString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.deref())
    }
}

pub const NOTO_SANS: &'static [u8] = include_bytes!("noto-sans.ttf");

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
