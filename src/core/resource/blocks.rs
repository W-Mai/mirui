//! Shared source blocks. Payload ownership survives both eviction and pending I/O.

use super::{AssetBuffer, AssetError};
use crate::core::cache::{Entry, Handle, HasSize, LruLinearCache, MaxSize};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) struct BlockKey {
    pub source: u64,
    pub offset: u64,
}

pub(super) struct Block {
    buffer: Option<AssetBuffer>,
    key: BlockKey,
    filled: usize,
    eof: bool,
    capacity: usize,
}

impl HasSize for Block {
    fn cache_size(&self) -> usize {
        self.capacity
    }
}

impl Block {
    pub fn bytes(&self) -> &[u8] {
        &self
            .buffer
            .as_ref()
            .expect("published block owns storage")
            .bytes[..self.filled]
    }

    pub fn key(&self) -> BlockKey {
        self.key
    }
    pub fn eof(&self) -> bool {
        self.eof
    }
}

/// A reservation stays occupied until the original physical operation returns its buffer.
pub(super) struct BlockWrite {
    slot: usize,
    key: BlockKey,
    address: usize,
}

pub(super) struct BlockPool {
    cache: LruLinearCache<BlockKey, Block>,
    slots: Vec<Option<Handle<Block>>>,
    block_size: usize,
    in_flight: usize,
    max_in_flight: usize,
    resident: bool,
    next_source: u64,
}

impl BlockPool {
    /// Preparation allocates all payload blocks and warms the bounded cache metadata once.
    pub fn new(
        block_size: usize,
        budget: MaxSize,
        max_in_flight: usize,
    ) -> Result<Self, AssetError> {
        if block_size == 0 || max_in_flight == 0 {
            return Err(AssetError::Capacity);
        }
        let count = match budget {
            MaxSize::Bytes(bytes) => bytes / block_size,
            MaxSize::Count(count) => count,
            MaxSize::Disabled => max_in_flight,
            MaxSize::Unbound => return Err(AssetError::Capacity),
        };
        if count < max_in_flight || count.checked_mul(block_size).is_none() {
            return Err(AssetError::Capacity);
        }
        let mut cache = LruLinearCache::new(MaxSize::Count(count));
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| AssetError::Capacity)?;
        for i in 0..count {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(block_size)
                .map_err(|_| AssetError::Capacity)?;
            bytes.resize(block_size, 0);
            let key = BlockKey {
                source: 0,
                offset: u64::try_from(i).map_err(|_| AssetError::Capacity)?,
            };
            let block = Block {
                buffer: Some(AssetBuffer {
                    bytes: bytes.into_boxed_slice(),
                    start: 0,
                    end: block_size,
                }),
                key,
                filled: 0,
                eof: false,
                capacity: block_size,
            };
            slots.push(Some(cache.entry(key).or_insert_with(|| block)));
        }
        // Unlike clear(), repeated eviction does not allocate a temporary id vector.
        while cache.evict_one().is_some() {}
        Ok(Self {
            cache,
            slots,
            block_size,
            in_flight: 0,
            max_in_flight,
            resident: budget != MaxSize::Disabled,
            next_source: 1,
        })
    }

    pub fn new_source(&mut self) -> Result<u64, AssetError> {
        let id = self.next_source;
        self.next_source = id.checked_add(1).ok_or(AssetError::Capacity)?;
        Ok(id)
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn acquire(&mut self, source: u64, offset: u64) -> Option<Handle<Block>> {
        let key = self.cache.iter().find_map(|(key, block)| {
            let end = key.offset.checked_add(u64::try_from(block.filled).ok()?)?;
            (key.source == source
                && offset >= key.offset
                && (offset < end || (offset == end && block.eof)))
                .then_some(*key)
        })?;
        self.cache.acquire(&key)
    }

    pub fn begin(&mut self, key: BlockKey) -> Result<(BlockWrite, AssetBuffer), AssetError> {
        if self.in_flight == self.max_in_flight {
            return Err(AssetError::Capacity);
        }
        let slot = loop {
            if let Some(slot) = self.slots.iter_mut().position(|owner| {
                owner
                    .as_mut()
                    .and_then(Handle::get_mut)
                    .is_some_and(|block| block.buffer.is_some())
            }) {
                break slot;
            }
            // Pinned evictees remain in slots and continue charging payload capacity.
            if self.cache.evict_one().is_none() {
                return Err(AssetError::Capacity);
            }
        };
        let block = self.slots[slot].as_mut().unwrap().get_mut().unwrap();
        let mut buffer = block.buffer.take().unwrap();
        buffer.start = 0;
        buffer.end = self.block_size;
        block.key = key;
        block.filled = 0;
        block.eof = false;
        let write = BlockWrite {
            slot,
            key,
            address: buffer.bytes.as_ptr() as usize,
        };
        self.in_flight += 1;
        Ok((write, buffer))
    }

    pub fn finish(
        &mut self,
        write: BlockWrite,
        buffer: AssetBuffer,
        result: Result<(usize, bool), AssetError>,
    ) -> Result<Handle<Block>, AssetError> {
        self.restore(&write, buffer)?;
        let owner = self.slots[write.slot].as_mut().unwrap();
        let block = owner.get_mut().unwrap();
        let (filled, eof) = result?;
        if filled > self.block_size {
            return Err(AssetError::InvalidSource);
        }
        block.filled = filled;
        block.eof = eof;
        block.key = write.key;
        if !self.resident {
            return Ok(owner.clone());
        }
        let owner = self.slots[write.slot].take().unwrap();
        let owner = match self.cache.entry(write.key) {
            Entry::Vacant(entry) => entry
                .reinsert(owner)
                .unwrap_or_else(|_| unreachable!("prepared cache admission")),
            Entry::Occupied(_) => {
                self.slots[write.slot] = Some(owner);
                return Err(AssetError::InvalidSource);
            }
        };
        self.slots[write.slot] = Some(owner.clone());
        Ok(owner)
    }

    pub fn abort(&mut self, write: BlockWrite, buffer: AssetBuffer) -> Result<(), AssetError> {
        self.restore(&write, buffer)
    }

    /// An unwound provider did not return its buffer. Keep that payload slot unavailable.
    pub fn abandon(&mut self, _write: BlockWrite) {
        self.in_flight -= 1;
    }

    fn restore(&mut self, write: &BlockWrite, buffer: AssetBuffer) -> Result<(), AssetError> {
        self.in_flight -= 1;
        if buffer.bytes.as_ptr() as usize != write.address || buffer.bytes.len() != self.block_size
        {
            // A broken provider returned a different operation's storage. Do not fabricate replacement capacity.
            return Err(AssetError::InvalidSource);
        }
        let owner = self.slots[write.slot]
            .as_mut()
            .expect("reserved block owner");
        owner
            .get_mut()
            .expect("in-flight block is exclusive")
            .buffer = Some(buffer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_budget_counts_pending_and_pinned_evictions_without_hot_allocation() {
        let mut pool = BlockPool::new(8, MaxSize::Bytes(16), 2).unwrap();
        let source = pool.new_source().unwrap();
        let allocations = crate::test_support::tracked_allocations(|| {
            let (a, mut bytes) = pool.begin(BlockKey { source, offset: 0 }).unwrap();
            let address = bytes.as_slice().as_ptr();
            bytes.as_mut_slice().copy_from_slice(b"abcdefgh");
            let old = pool.finish(a, bytes, Ok((8, false))).unwrap();
            let (b, bytes) = pool.begin(BlockKey { source, offset: 8 }).unwrap();
            assert!(matches!(
                pool.begin(BlockKey { source, offset: 16 }),
                Err(AssetError::Capacity)
            ));
            assert_eq!(old.bytes(), b"abcdefgh");
            assert_eq!(old.bytes().as_ptr(), address);
            let second = pool.finish(b, bytes, Ok((8, true))).unwrap();
            assert!(second.eof());
            drop(old);
            let (c, bytes) = pool.begin(BlockKey { source, offset: 16 }).unwrap();
            assert_eq!(bytes.as_slice().as_ptr(), address);
            assert!(
                pool.finish(c, bytes, Err(AssetError::Io("read failed")))
                    .is_err()
            );
            assert_eq!(second.key().offset, 8);
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn opened_identity_and_disabled_cache_preserve_boundaries() {
        let mut pool = BlockPool::new(4, MaxSize::Count(2), 1).unwrap();
        let a = pool.new_source().unwrap();
        let b = pool.new_source().unwrap();
        let (write, bytes) = pool
            .begin(BlockKey {
                source: a,
                offset: 4,
            })
            .unwrap();
        let lease = pool.finish(write, bytes, Ok((3, true))).unwrap();
        assert_eq!(pool.acquire(a, 5).unwrap().bytes().len(), 3);
        assert!(pool.acquire(a, 7).unwrap().eof());
        assert!(pool.acquire(a, 8).is_none());
        assert!(pool.acquire(b, 5).is_none());
        drop(lease);
        let mut pool = BlockPool::new(4, MaxSize::Disabled, 1).unwrap();
        assert_eq!(pool.block_size(), 4);
        let (write, bytes) = pool
            .begin(BlockKey {
                source: a,
                offset: 0,
            })
            .unwrap();
        let lease = pool.finish(write, bytes, Ok((4, false))).unwrap();
        assert!(pool.acquire(a, 0).is_none());
        assert!(
            pool.begin(BlockKey {
                source: a,
                offset: 4
            })
            .is_err()
        );
        drop(lease);
        assert!(
            pool.begin(BlockKey {
                source: a,
                offset: 4
            })
            .is_ok()
        );
    }
}
