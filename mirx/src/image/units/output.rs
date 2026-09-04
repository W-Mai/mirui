use super::{UnitMemoryPlan, UnitPlane, UnitPlanes};

/// Sequential logical bytes mapped into checked physical plane rows.
pub(super) struct UnitOutput<'a> {
    memory: UnitMemoryPlan,
    output: &'a mut [u8],
    planes: UnitPlanes,
    plane: Option<UnitPlane>,
    row: u32,
    column: usize,
}
impl<'a> UnitOutput<'a> {
    pub(super) fn new(memory: UnitMemoryPlan, output: &'a mut [u8]) -> Self {
        output.fill(0);
        let mut writer = Self {
            memory,
            output,
            planes: memory.planes(),
            plane: None,
            row: 0,
            column: 0,
        };
        writer.next_plane();
        writer
    }
    fn next_plane(&mut self) {
        self.plane = self
            .planes
            .find(|plane| plane.geometry().width() != 0 && plane.geometry().height() != 0);
        self.row = 0;
        self.column = 0;
    }
    fn row_remaining(&self) -> usize {
        self.plane
            .expect("remaining decoded plane")
            .geometry()
            .minimum_stride()
            .expect("sample row") as usize
            - self.column
    }
    fn position(&self) -> usize {
        let memory = self.plane.expect("remaining decoded plane").memory();
        memory.data_offset() as usize + self.row as usize * memory.stride() as usize + self.column
    }
    fn advance(&mut self, count: usize) {
        let plane = self.plane.expect("remaining decoded plane");
        self.column += count;
        if self.column == plane.geometry().minimum_stride().expect("sample row") as usize {
            self.column = 0;
            self.row += 1;
            if self.row == plane.geometry().height() {
                self.next_plane();
            }
        }
    }
    pub(super) fn write(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let count = self.row_remaining().min(bytes.len());
            let start = self.position();
            self.output[start..start + count].copy_from_slice(&bytes[..count]);
            self.advance(count);
            bytes = &bytes[count..];
        }
    }
    pub(super) fn repeat(&mut self, bytes: &[u8], mut repeat: usize) {
        while repeat > 0 {
            let count = repeat.min(self.row_remaining() / bytes.len());
            if count == 0 {
                self.write(bytes);
                repeat -= 1;
            } else {
                let start = self.position();
                let length = count * bytes.len();
                for target in self.output[start..start + length].chunks_exact_mut(bytes.len()) {
                    target.copy_from_slice(bytes);
                }
                self.advance(length);
                repeat -= count;
            }
        }
    }
    pub(super) fn finish(self) {
        debug_assert!(self.plane.is_none());
        for plane in self.memory.planes() {
            let geometry = plane.geometry();
            let mask = geometry.row_tail_mask();
            if mask == 0xff {
                continue;
            }
            let row_len = geometry.minimum_stride().expect("sample row") as usize;
            let memory = plane.memory();
            for row in 0..geometry.height() {
                let last = memory.data_offset() as usize
                    + row as usize * memory.stride() as usize
                    + row_len
                    - 1;
                self.output[last] &= mask;
            }
        }
    }
}
