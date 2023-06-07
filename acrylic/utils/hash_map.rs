use ahash::random_state::RandomState;
use core::{marker::PhantomData, hash::{Hash, BuildHasher, Hasher as _}};
use crate::LiteMap;

static SEED: &'static [u8] = include_bytes!(concat!(env!("OUT_DIR"), "/seed.dat"));

macro_rules! seed {
    ($i:literal) => ( [
        SEED[$i + 0],
        SEED[$i + 1],
        SEED[$i + 2],
        SEED[$i + 3],
        SEED[$i + 4],
        SEED[$i + 5],
        SEED[$i + 6],
        SEED[$i + 7],
    ] )
}

static GEN: RandomState = RandomState::with_seeds(
    u64::from_ne_bytes(seed!( 0)),
    u64::from_ne_bytes(seed!( 8)),
    u64::from_ne_bytes(seed!(16)),
    u64::from_ne_bytes(seed!(24)),
);

#[derive(Clone, Default)]
pub struct HashMap<K, V>(LiteMap<u64, V>, PhantomData<K>);

impl<K: Hash + Ord, V> HashMap<K, V> {
    pub fn new() -> Self {
        Self(LiteMap::new(), PhantomData)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut hasher = GEN.build_hasher();
        key.hash(&mut hasher);
        self.0.insert(hasher.finish(), value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        let mut hasher = GEN.build_hasher();
        key.hash(&mut hasher);
        self.0.contains_key(&hasher.finish())
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut hasher = GEN.build_hasher();
        key.hash(&mut hasher);
        self.0.get(&hasher.finish())
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let mut hasher = GEN.build_hasher();
        key.hash(&mut hasher);
        self.0.get_mut(&hasher.finish())
    }
}
