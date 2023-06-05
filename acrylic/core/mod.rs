//! `acrylic` internals: Events, Layout, JSON State, XML Parsing, ...

pub mod app;
pub mod event;
pub mod glyph;
pub mod layout;
pub mod node;
pub mod state;
pub mod style;
pub mod visual;
pub mod xml;

use super::{CheapString, Vec};
use core::ops::Deref;

pub use oakwood::for_each_child;
pub use rgb;

/// General-purpose (Key, Value) pair
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyValue {
    pub key: CheapString,
    pub value: CheapString,
}

/// General-purpose Vector of (Key, Value) pairs
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct KeyValueStore {
    vec: Vec<KeyValue>,
}

impl KeyValueStore {
    pub const fn new() -> Self {
        Self {
            vec: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: Vec::with_capacity(capacity),
        }
    }

    pub fn push<K: Into<CheapString>, V: Into<CheapString>>(&mut self, key: K, value: V) {
        self.vec.push(KeyValue {
            key: key.into(),
            value: value.into(),
        })
    }

    pub fn find(&self, key: &str) -> Option<usize> {
        let mut i = 0;
        for kv in &self.vec {
            if kv.key.deref() == key {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    pub fn get<'a>(&'a self, key: &str) -> Option<&'a CheapString> {
        Some(&self.vec[self.find(key)?].value)
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn iter(&self) -> KeyValueStoreIter {
        KeyValueStoreIter {
            kvs: self,
            index: 0,
        }
    }
}

pub struct KeyValueStoreIter<'a> {
    kvs: &'a KeyValueStore,
    index: usize,
}

impl<'a> Iterator for KeyValueStoreIter<'a> {
    type Item = (&'a CheapString, &'a CheapString);
    fn next(&mut self) -> Option<Self::Item> {
        // Increment our count. This is why we started at zero.
        let entry: &KeyValue = self.kvs.vec.get(self.index)?;
        self.index += 1;
        Some((&entry.key, &entry.value))
    }
}
