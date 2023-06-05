//! JSON State Store

use crate::{Error, error, CheapString, Hasher, HashMap};
use super::app::Application;
use super::visual::{Pixels, Ratio};
use super::node::NodeKey;
pub use serde_json::Value as StateValue;
use core::{fmt::{self, Write}, write, str::Split, ops::Deref};

/// Parse a JSON string into a JSON State
pub fn parse_state(json: &str) -> Result<StateValue, Error> {
    match serde_json::from_str(json) {
        Ok(value) => Ok(value),
        Err(e) => Err(error!("JSON state parsing error: {:?}", e)),
    }
}

/// A unique value identifying a JSON state path
pub type StatePathHash = u64;

/// One step in a JSON state path
pub enum StatePathStep<'a> {
    Key(&'a str),
    Index(usize),
}

/// The result of a JSON state lookup
#[derive(Clone)]
pub enum StateFinderResult {
    String(CheapString),
    Boolean(bool),
    Number(f32),
}

impl StateFinderResult {
    pub fn as_bool(&self) -> Result<bool, Error> {
        let msg = "Invalid boolean value";
        match self {
            Self::String(s) => match s.deref() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(error!("{}", msg)),
            },
            Self::Boolean(b) => Ok(*b),
            Self::Number(_) => Err(error!("{}", msg)),
        }
    }

    pub fn as_f32(self) -> Result<f32, Error> {
        let msg = "Invalid float value";
        match self {
            Self::String(s) => match s.deref().parse() {
                Ok(pixels) => Ok(pixels),
                Err(e) => Err(error!("{}: {:?}", msg, e)),
            },
            Self::Boolean(_) => Err(error!("{}", msg)),
            Self::Number(float) => Ok(float),
        }
    }

    pub fn as_usize(self) -> Result<usize, Error> {
        let msg = "Invalid unsigned integer value";
        match self {
            Self::String(s) => match s.deref().parse() {
                Ok(uint) => Ok(uint),
                Err(e) => Err(error!("{}: {:?}", msg, e)),
            },
            Self::Boolean(_) => Err(error!("{}", msg)),
            Self::Number(float) => match (float.is_finite(), float.fract() == 0.0) {
                (true, true) => Ok(float as _),
                _ => Err(error!("{}", msg)),
            },
        }
    }

    pub fn as_str(self) -> Result<CheapString, Error> {
        let msg = "Invalid string value";
        match self {
            Self::String(s) => Ok(s),
            Self::Boolean(_) => Err(error!("{}", msg)),
            Self::Number(_) => Err(error!("{}", msg)),
        }
    }

    /// Counts the number of characters this value would take, if converted to a string
    pub fn display_len(&self) -> usize {
        let mut counter = CharCounter(0);
        write!(&mut counter, "{}", self).unwrap();
        counter.0
    }

    pub fn as_pixels(self) -> Result<Pixels, Error> {
        Ok(Pixels::from_num(self.as_f32()?))
    }

    pub fn as_ratio(self) -> Result<Ratio, Error> {
        Ok(Ratio::from_num(self.as_f32()?))
    }

    /// Tries to split this value at each whitespace character.
    ///
    /// If this value isn't a string, the returned iterator will yield the value (once).
    pub fn split_space(&self) -> SpaceIterator {
        match self {
            Self::String(s) => SpaceIterator::String(s.split_space()),
            _ => SpaceIterator::Other(self.clone(), false),
        }
    }
}

/// See [`StateFinderResult::split_space`]
pub enum SpaceIterator<'a> {
    String(Split<'a, char>),
    Other(StateFinderResult, bool),
}

pub enum SpaceIteratorResult<'a> {
    String(&'a str),
    Boolean(bool),
    Number(f32),
}

impl<'a> Iterator for SpaceIterator<'a> {
    type Item = SpaceIteratorResult<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::String(split) => Some(SpaceIteratorResult::String(split.next()?)),
            Self::Other(other, done) => {
                if *done {
                    None
                } else {
                    *done = true;
                    Some(match other {
                        StateFinderResult::String(_) => unreachable!(),
                        StateFinderResult::Boolean(b) => SpaceIteratorResult::Boolean(*b),
                        StateFinderResult::Number(f) => SpaceIteratorResult::Number(*f),
                    })
                }
            },
        }
    }
}

impl fmt::Display for StateFinderResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{}", s.deref()),
            Self::Boolean(b) => write!(f, "{}", b),
            Self::Number(float) => write!(f, "{}", float),
        }
    }
}

impl<'a> fmt::Display for SpaceIteratorResult<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{}", s.deref()),
            Self::Boolean(b) => write!(f, "{}", b),
            Self::Number(float) => write!(f, "{}", float),
        }
    }
}

/// A callback function used as a custom State lookup function.
///
/// When a secondary state store (namespace) is created by a node (the "masking" node),
/// A function with this signature is stored, which allows the masking node
/// to customize the lookup policy for this state store. This is currently
/// only used by Iterating Containers (see the `generator` state lookup function
/// in container code).
///
/// As all state lookups performed by (indirect) children of the masking node
/// will go through this function, it should fall back to the default
/// [`Application::state_lookup`] method if the requested state store
/// isn't the new one:
/// ```rust
/// if store == "my-custom-store" {
///     // perform your custom lookup
/// } else {
///     // act as if the masking node
///     app.state_lookup(masker, store, key, path_hash)
/// }
/// ```
pub type StateFinder = for<'a> fn(
    app: &'a mut Application,
    masker: NodeKey,
    node: NodeKey,
    store: &str,
    key: &str,
    path_hash: &mut Hasher,
) -> Result<&'a mut StateValue, Error>;

/// Map of nodes which created custom state stores
pub type StateMasks = HashMap<NodeKey, StateFinder>;

/// Parse a string as a list of path steps
pub fn path_steps(path: &str) -> impl Iterator<Item = StatePathStep> {
    path.split('.').filter(|v| v.len() > 0).map(|s| {
        match s.parse::<usize>() {
            Ok(index) => StatePathStep::Index(index),
            Err(_) => StatePathStep::Key(s),
        }
    })
}

struct CharCounter(usize);

impl fmt::Write for CharCounter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}
