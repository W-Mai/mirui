#[derive(Debug, Default, Clone, Copy)]
pub struct CacheStats {
    pub hit_count: u64,
    pub miss_count: u64,
    pub evict_count: u64,
    pub insert_count: u64,
    pub drop_count: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f32 {
        let total = self.hit_count + self.miss_count;
        if total == 0 {
            0.0
        } else {
            self.hit_count as f32 / total as f32
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
