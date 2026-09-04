use super::{UnitMemoryPlan, UnitPlane, UnitPlanes};

/// Sequential logical bytes mapped into checked physical plane rows.
pub(super) struct UnitOutput<'a> {
    memory: UnitMemoryPlan,
    output: &'a mut [u8],
    cursor: RowCursor,
    written: usize,
}

struct RowCursor {
    planes: UnitPlanes,
    plane: Option<UnitPlane>,
    row: u32,
    column: usize,
}
impl RowCursor {
    fn at(memory: UnitMemoryPlan, mut position: usize) -> Self {
        let mut cursor = Self {
            planes: memory.planes(),
            plane: None,
            row: 0,
            column: 0,
        };
        cursor.next_plane();
        while let Some(plane) = cursor.plane {
            let row_len = plane.geometry().minimum_stride().expect("sample row") as usize;
            let len = row_len * plane.geometry().height() as usize;
            if position < len {
                cursor.row = (position / row_len) as u32;
                cursor.column = position % row_len;
                return cursor;
            }
            position -= len;
            cursor.next_plane();
        }
        debug_assert_eq!(position, 0);
        cursor
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
}

impl<'a> UnitOutput<'a> {
    pub(super) fn new(memory: UnitMemoryPlan, output: &'a mut [u8]) -> Self {
        output.fill(0);
        Self {
            memory,
            output,
            cursor: RowCursor::at(memory, 0),
            written: 0,
        }
    }
    fn advance(&mut self, count: usize) {
        self.written += count;
        self.cursor.advance(count);
    }
    pub(super) fn write(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let count = self.cursor.row_remaining().min(bytes.len());
            let start = self.cursor.position();
            self.output[start..start + count].copy_from_slice(&bytes[..count]);
            self.advance(count);
            bytes = &bytes[count..];
        }
    }
    pub(super) fn repeat(&mut self, bytes: &[u8], mut repeat: usize) {
        while repeat > 0 {
            let count = repeat.min(self.cursor.row_remaining() / bytes.len());
            if count == 0 {
                self.write(bytes);
                repeat -= 1;
            } else {
                let start = self.cursor.position();
                let length = count * bytes.len();
                let target = &mut self.output[start..start + length];
                if bytes.len() == 1 {
                    target.fill(bytes[0]);
                } else {
                    for target in target.chunks_exact_mut(bytes.len()) {
                        target.copy_from_slice(bytes);
                    }
                }
                self.advance(length);
                repeat -= count;
            }
        }
    }
    pub(super) fn copy_match(&mut self, distance: u16, mut len: usize) {
        let distance = usize::from(distance);
        let mut source = RowCursor::at(self.memory, self.written - distance);
        if distance <= 4 {
            let mut period = [0; 4];
            for byte in &mut period[..distance] {
                *byte = self.output[source.position()];
                source.advance(1);
            }
            self.repeat(&period[..distance], len / distance);
            self.write(&period[..len % distance]);
        } else {
            while len > 0 {
                let count = len
                    .min(distance)
                    .min(source.row_remaining())
                    .min(self.cursor.row_remaining());
                let start = source.position();
                self.output
                    .copy_within(start..start + count, self.cursor.position());
                source.advance(count);
                self.advance(count);
                len -= count;
            }
        }
    }
    pub(super) fn finish(self) {
        debug_assert!(self.cursor.plane.is_none());
        debug_assert_eq!(self.written, self.memory.sample_byte_len());
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

    /// Expands tight selected-plane bytes in place into their planned rows.
    ///
    /// Destinations never precede their tight source positions because planned
    /// strides and inter-plane offsets can only add padding. Reverse traversal
    /// therefore preserves every unread source byte while overlapping moves are
    /// performed. All physical gaps are cleared after placement.
    pub(super) fn expand_tight(memory: UnitMemoryPlan, output: &mut [u8]) {
        debug_assert!(output.len() >= memory.byte_len() as usize);
        let mut source_end = memory.sample_byte_len();
        for plane in memory.planes().rev() {
            let geometry = plane.geometry();
            let row_len = geometry.minimum_stride().expect("sample row") as usize;
            let plane_len = row_len * geometry.height() as usize;
            let source_start = source_end - plane_len;
            let target = plane.memory();
            for row in (0..geometry.height() as usize).rev() {
                let source = source_start + row * row_len;
                let destination = target.data_offset() as usize + row * target.stride() as usize;
                output.copy_within(source..source + row_len, destination);
            }
            source_end = source_start;
        }
        debug_assert_eq!(source_end, 0);

        let mut clear_from = 0;
        for plane in memory.planes() {
            let geometry = plane.geometry();
            let row_len = geometry.minimum_stride().expect("sample row") as usize;
            let target = plane.memory();
            for row in 0..geometry.height() as usize {
                let row_start = target.data_offset() as usize + row * target.stride() as usize;
                output[clear_from..row_start].fill(0);
                clear_from = row_start + row_len;
            }
        }
        output[clear_from..memory.byte_len() as usize].fill(0);
    }
}
