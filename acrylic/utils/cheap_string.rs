use ::core::{fmt, str::Split, ops::Deref, hash::Hash, cmp::Ordering};
use crate::{String, Rc};

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

impl PartialOrd for CheapString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CheapString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(&other.deref())
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
