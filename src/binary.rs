use bytemuck::Pod;

pub struct BlobBuilder {
    pub buffer: Vec<u8>,
}

impl BlobBuilder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn align<T: Pod>(&mut self) {
        let align = std::mem::align_of::<T>();
        let pad = (align - (self.buffer.len() % align)) % align;
        self.buffer.extend(std::iter::repeat(0u8).take(pad));
    }

    pub fn reserve<T: Pod>(&mut self) -> usize {
        self.align::<T>();
        let offset = self.buffer.len();
        self.buffer.resize(offset + std::mem::size_of::<T>(), 0);
        offset
    }

    pub fn push_slice<T: Pod>(&mut self, data: &[T]) -> usize {
        self.align::<T>();
        let offset = self.buffer.len();
        self.buffer.extend_from_slice(bytemuck::cast_slice(data));
        offset
    }

    pub fn push_bytes(&mut self, data: &[u8]) -> usize {
        let offset = self.buffer.len();
        self.buffer.extend_from_slice(data);
        offset
    }

    pub fn write_at<T: Pod>(&mut self, offset: usize, value: &T) {
        let bytes = bytemuck::bytes_of(value);
        let end = offset + bytes.len();
        self.buffer[offset..end].copy_from_slice(bytes);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buffer
    }
}