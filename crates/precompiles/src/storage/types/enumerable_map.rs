//! Minimal enumerable map for EVM storage.
//!
//! This stores an authoritative `Mapping<K, V>` alongside a `Vec<K>` index used only for
//! enumeration and bounded cleanup. It deliberately does not maintain an auxiliary positions
//! mapping, so insert/remove on the key index are O(n) scans over the key list.
//!
//! # Storage Layout
//!
//! EnumerableMap uses two storage structures:
//! - **Keys Vec**: a `Vec<K>` storing the enumerated keys at `base_slot`
//! - **Values Mapping**: a `Mapping<K, V>` at `base_slot + 1`
//!
//! # Design
//!
//! - Reads on hot paths should go through the mapping, e.g. `map[key].field.read()?`
//! - The key vector is only for cold-path enumeration and cleanup
//! - Mutations to mapped values and the key index are intentionally separate so callers can
//!   decide how liveness is represented in `V`

use alloy::primitives::{Address, U256};
use std::{
    fmt,
    hash::Hash,
    ops::{Index, IndexMut},
};

use crate::{
    error::{Result, TempoPrecompileError},
    storage::{
        Handler, Layout, LayoutCtx, Mapping, Storable, StorableType, StorageKey, vec::VecHandler,
    },
};

/// Enumerable map storage primitive backed by `Vec<K> + Mapping<K, V>`.
pub struct EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    keys: VecHandler<K>,
    values: Mapping<K, V>,
    base_slot: U256,
    address: Address,
}

impl<K, V> EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    /// Creates a new enumerable map handler for the given base slot.
    #[inline]
    pub fn new(base_slot: U256, address: Address) -> Self {
        Self {
            keys: VecHandler::new(base_slot, address),
            values: Mapping::new(base_slot + U256::ONE, address),
            base_slot,
            address,
        }
    }

    /// Returns the base storage slot for this map.
    #[inline]
    pub fn base_slot(&self) -> U256 {
        self.base_slot
    }

    /// Returns the number of enumerated keys.
    #[inline]
    pub fn len(&self) -> Result<usize> {
        self.keys.len()
    }

    /// Returns whether the enumerated key index is empty.
    #[inline]
    pub fn is_empty(&self) -> Result<bool> {
        self.keys.is_empty()
    }

    /// Returns the enumerated keys in insertion order.
    #[inline]
    pub fn keys(&self) -> Result<Vec<K>> {
        self.keys.read()
    }

    /// Returns true if the key exists in the enumerable index.
    #[inline]
    pub fn contains_key(&self, key: &K) -> Result<bool> {
        Ok(self.keys()?.contains(key))
    }

    /// Returns whether the mapped value for `key` should be considered present.
    ///
    /// This is for value types that encode liveness in the mapped payload itself,
    /// e.g. a `mode` field where `0` means absent and non-zero means present.
    /// The enumerable key index is not consulted.
    #[inline]
    pub fn contains_mapped<F>(&self, key: &K, is_present: F) -> Result<bool>
    where
        F: FnOnce(&V::Handler) -> Result<bool>,
    {
        is_present(self.at(key))
    }

    /// Adds a key to the enumerable index if it is not already present.
    ///
    /// Returns `true` when the key was inserted into the index.
    #[inline]
    pub fn insert_key(&mut self, key: K) -> Result<bool>
    where
        K::Handler: Handler<K>,
    {
        if self.contains_key(&key)? {
            return Ok(false);
        }

        self.keys.push(key)?;
        Ok(true)
    }

    /// Appends a key to the enumerable index without checking for duplicates.
    ///
    /// Callers must only use this after proving absence from the authoritative
    /// mapping or otherwise guaranteeing uniqueness.
    #[inline]
    pub fn insert_key_unchecked(&self, key: K) -> Result<()>
    where
        K::Handler: Handler<K>,
    {
        self.keys.push(key)
    }

    /// Removes a key from the enumerable index.
    ///
    /// This only updates the key index. Callers remain responsible for clearing any mapped value.
    #[inline]
    pub fn remove_key(&mut self, key: &K) -> Result<bool> {
        let mut keys = self.keys()?;
        let original_len = keys.len();
        keys.retain(|existing| existing != key);

        if keys.len() == original_len {
            return Ok(false);
        }

        self.keys.write(keys)?;
        Ok(true)
    }

    /// Clears the enumerable key index without touching mapped values.
    #[inline]
    pub fn clear_keys(&mut self) -> Result<()> {
        self.keys.delete()
    }

    /// Returns the value handler for the given key.
    #[inline]
    pub fn at(&self, key: &K) -> &V::Handler {
        self.values.at(key)
    }

    /// Returns the mutable value handler for the given key.
    #[inline]
    pub fn at_mut(&mut self, key: &K) -> &mut V::Handler {
        self.values.at_mut(key)
    }
}

impl<K, V> Default for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    fn default() -> Self {
        Self::new(U256::ZERO, Address::ZERO)
    }
}

impl<K, V> Storable for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    fn load<S: crate::storage::StorageOps>(
        _storage: &S,
        _slot: U256,
        _ctx: LayoutCtx,
    ) -> Result<Self> {
        Err(TempoPrecompileError::Fatal(
            "EnumerableMap must be accessed through its generated handler".into(),
        ))
    }

    fn store<S: crate::storage::StorageOps>(
        &self,
        _storage: &mut S,
        _slot: U256,
        _ctx: LayoutCtx,
    ) -> Result<()> {
        Err(TempoPrecompileError::Fatal(
            "EnumerableMap must be accessed through its generated handler".into(),
        ))
    }

    fn delete<S: crate::storage::StorageOps>(
        _storage: &mut S,
        _slot: U256,
        _ctx: LayoutCtx,
    ) -> Result<()> {
        Err(TempoPrecompileError::Fatal(
            "EnumerableMap must be accessed through its generated handler".into(),
        ))
    }
}

impl<K, V> Index<K> for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    type Output = V::Handler;

    #[inline]
    fn index(&self, key: K) -> &Self::Output {
        &self.values[key]
    }
}

impl<K, V> IndexMut<K> for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    #[inline]
    fn index_mut(&mut self, key: K) -> &mut Self::Output {
        &mut self.values[key]
    }
}

impl<K, V> StorableType for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    const LAYOUT: Layout = Layout::Slots(2);
    type Handler = Self;

    fn handle(slot: U256, _ctx: LayoutCtx, address: Address) -> Self::Handler {
        Self::new(slot, address)
    }
}

impl<K, V> fmt::Debug for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnumerableMap")
            .field("base_slot", &self.base_slot)
            .field("address", &self.address)
            .finish()
    }
}

impl<K, V> Clone for EnumerableMap<K, V>
where
    K: Storable + StorageKey + Hash + Eq + Clone,
    V: StorableType,
{
    fn clone(&self) -> Self {
        Self::new(self.base_slot, self.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::{Handler, StorageCtx},
        test_util::setup_storage,
    };

    #[test]
    fn test_enumerable_map_key_index_preserves_order_and_uniqueness() {
        let (mut storage, address) = setup_storage();
        StorageCtx::enter(&mut storage, || -> Result<()> {
            let mut map = EnumerableMap::<Address, u8>::new(U256::ZERO, address);
            let first = Address::repeat_byte(0x11);
            let second = Address::repeat_byte(0x22);

            assert!(map.insert_key(first)?);
            assert!(map.insert_key(second)?);
            assert!(!map.insert_key(first)?);

            assert_eq!(map.keys()?, vec![first, second]);
            assert!(map.contains_key(&first)?);
            assert_eq!(map.len()?, 2);

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_enumerable_map_remove_key_only_updates_index() {
        let (mut storage, address) = setup_storage();
        StorageCtx::enter(&mut storage, || -> Result<()> {
            let mut map = EnumerableMap::<Address, u8>::new(U256::ZERO, address);
            let key = Address::repeat_byte(0x33);

            assert!(map.insert_key(key)?);
            map[key].write(7)?;

            assert!(map.remove_key(&key)?);
            assert!(!map.contains_key(&key)?);
            assert!(map.keys()?.is_empty());
            assert_eq!(map[key].read()?, 7);

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_enumerable_map_contains_mapped_uses_value_liveness() {
        let (mut storage, address) = setup_storage();
        StorageCtx::enter(&mut storage, || -> Result<()> {
            let key = Address::repeat_byte(0x44);
            let mut map = EnumerableMap::<Address, u8>::new(U256::ZERO, address);

            assert!(!map.contains_mapped(&key, |value| value.read().map(|mode| mode != 0))?);

            map[key].write(2)?;
            assert!(map.contains_mapped(&key, |value| value.read().map(|mode| mode != 0))?);

            map[key].write(0)?;
            assert!(!map.contains_mapped(&key, |value| value.read().map(|mode| mode != 0))?);

            Ok(())
        })
        .unwrap();
    }
}
