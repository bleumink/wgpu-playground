use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::context::RenderContext;

#[derive(Clone, Serialize, Deserialize)]
pub struct Instance {
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
}

impl Instance {
    pub fn new(position: glam::Vec3, rotation: glam::Quat) -> Self {
        Self { position, rotation }
    }

    pub fn to_mat4(&self) -> glam::Mat4 {
        glam::Mat4::from_rotation_translation(self.rotation, self.position)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RawInstance {
    pub transform_index: u32,
    pub material_index: u32,
}

impl RawInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<u32>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

// pub struct Instances {
//     pub data: Vec<RawInstance>,
//     pub buffer: wgpu::Buffer,
// }

// impl Instances {
//     pub fn new(instances: &[Instance], context: &RenderContext) -> Self {
//         let data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
//         let buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//             label: Some("Instance buffer"),
//             contents: bytemuck::cast_slice(&data),
//             usage: wgpu::BufferUsages::VERTEX,
//         });

//         Self { data, buffer }
//     }
// }

pub struct InstancePool {
    pub buffer: wgpu::Buffer,
    pub capacity: usize,
    pub cursor: usize,
}

impl InstancePool {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance pool"),
            size: (capacity * std::mem::size_of::<RawInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            capacity: capacity.max(1),
            cursor: 0,        
        }        
    }   

    pub fn upload(&mut self, instances: &[RawInstance], context: &RenderContext) -> usize {
        let size = instances.len();
        if self.cursor + size > self.capacity {
            self.cursor = 0;
        }

        let offset = (self.cursor * std::mem::size_of::<RawInstance>()) as u64;        
        context.queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(instances));
        
        let start_offset = self.cursor;
        self.cursor += size;

        start_offset
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}