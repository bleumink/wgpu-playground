use crate::context::RenderContext;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform([[f32; 4]; 4]);

impl TransformUniform {
    pub fn new(transform: glam::Mat4) -> Self {
        Self(transform.to_cols_array_2d())
    }

    pub fn identity() -> Self {
        Self(glam::Mat4::IDENTITY.to_cols_array_2d())
    }
}

pub struct TransformBuffer {
    transforms: Vec<TransformUniform>,
    capacity: usize,
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
}

impl TransformBuffer {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Transform bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let buffer = Self::create_buffer(capacity, context);
        let transforms = Vec::new();

        Self {
            transforms,
            capacity,
            buffer,
            layout,
        }
    }

    fn create_buffer(capacity: usize, context: &RenderContext) -> wgpu::Buffer {
        context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transform buffer"),
            size: (capacity * std::mem::size_of::<TransformUniform>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn write(&mut self, index: usize, matrix: glam::Mat4, context: &RenderContext) {
        if index >= self.transforms.len() {
            self.transforms.resize(index + 1, TransformUniform::identity());
        }

        let transform = TransformUniform::new(matrix);
        self.transforms[index] = transform;

        let offset = (index * std::mem::size_of::<TransformUniform>()) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&transform));
    }

    pub fn request_slot(&mut self, context: &RenderContext) -> usize {
        let index = self.transforms.len();
        if self.transforms.len() >= self.capacity {
            self.capacity *= 2;
            self.buffer = Self::create_buffer(self.capacity, context);

            context
                .queue
                .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.transforms));
        }

        self.transforms.push(TransformUniform::identity());
        index
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }
}
