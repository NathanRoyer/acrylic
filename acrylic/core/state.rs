//! JSON State

use crate::{Error, error, format, CheapString, HashMap, Vec, Hasher, LiteMap};
use super::app::Application;
use super::node::NodeKey;
pub use serde_json::Value as JsonValue;
use core::{fmt, hash::Hasher as _};

/// JSON state (internal representation)
#[derive(Debug, Clone)]
pub enum StateValue {
    Object(HashMap<str, StateValue>),
    Array(Vec<StateValue>),
    /// Numbers & booleans are converted to strings, since
    /// they're read as XML attribute values
    String(CheapString),
    Null,
}

impl StateValue {
    pub fn get_mut(&mut self, path: &str, path_hash: &mut Hasher) -> Result<&mut Self, Error> {
        let mut current = self;

        for path_step in path_steps(path) {
            let option = match path_step {
                StatePathStep::Index(index) => {
                    path_hash.write_usize(index);
                    match current {
                        Self::Array(array) => array.get_mut(index),
                        _ => return Err(error!("Invalid state path: can only index into array & objects"))
                    }
                },
                StatePathStep::Key(key) => {
                    path_hash.write(key.as_bytes());
                    match current {
                        Self::Object(obj) => obj.get_mut(key),
                        _ => return Err(error!("Invalid state path: cannot index into array & objects"))
                    }
                },
            };

            current = match option {
                Some(value) => value,
                None => return Err(error!("Invalid state path: {}", path)),
            }
        }

        Ok(current)
    }
}

fn map_value(value: JsonValue) -> StateValue {
    match value {
        JsonValue::Null => StateValue::Null,
        JsonValue::Array(mut a) => StateValue::Array(a.drain(..).map(|v| map_value(v)).collect()),
        JsonValue::Object(object) => StateValue::Object({
            let mut hash_map = HashMap::<str, StateValue>::new();

            for (key, value) in object {
                hash_map.insert_ref(&key, map_value(value));
            }

            hash_map
        }),

        JsonValue::Bool(b) => StateValue::String(match b {
            true => "true",
            false => "false",
        }.into()),
        JsonValue::Number(n) => StateValue::String(format!("{}", n).into()),
        JsonValue::String(s) => StateValue::String(s.into()),
    }
}

/// Parses serialized JSON into a JSON State
pub fn parse_state(json: &str) -> Result<StateValue, Error> {
    match serde_json::from_str(json) {
        Ok(value) => Ok(map_value(value)),
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

/// A callback function used as a custom State lookup function.
///
/// When a secondary state namespace is created by a node (the "masking" node),
/// A function with this signature is stored, which allows the masking node
/// to customize the lookup policy for this state namespace. This is currently
/// only used by Iterating Containers (see the `generator` state lookup function
/// in container code).
///
/// As all state lookups performed by (indirect) children of the masking node
/// will go through this function, it should fall back to the default
/// [`Application::state_lookup`] method if the requested state namespace
/// isn't the new one:
/// ```rust
/// if namespace == "my-custom-ns" {
///     // perform your custom lookup
/// } else {
///     // forward the lookup
///     app.state_lookup(masker, namespace, key, path_hash)
/// }
/// ```
pub type StateFinder = for<'a> fn(
    app: &'a mut Application,
    masker: NodeKey,
    node: NodeKey,
    namespace: &str,
    key: &str,
    path_hash: &mut Hasher,
) -> Result<&'a mut StateValue, Error>;

/// Map of nodes which created custom state namespaces
pub type StateMasks = LiteMap<NodeKey, StateFinder>;

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
