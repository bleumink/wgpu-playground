use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::{context::RenderContext, scene::SceneGraph, vertex::Vertex};

#[derive(Clone, Serialize, Deserialize)]
pub struct DemoInstance {
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
}

impl DemoInstance {
    pub fn new(position: glam::Vec3, rotation: glam::Quat) -> Self {
        Self { position, rotation }
    }

    pub fn to_mat4(&self) -> glam::Mat4 {
        glam::Mat4::from_rotation_translation(self.rotation, self.position)
    }
}

// pub trait Instanced {
//     type Instance: Pod + Vertex;

//     fn pipeline_id() -> &'static str;
//     fn instances(scene: &SceneGraph) -> Vec<Self::Instance>;
//     fn draw();
// }

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Instance {
    pub transform_index: u32,
    pub normal_index: u32,
}

impl Instance {
    pub const STRIDE: usize = std::mem::size_of::<Self>();
}

impl Vertex for Instance {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: Self::STRIDE as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<u32>() as u64,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

pub struct InstancePool {
    pub buffer: wgpu::Buffer,
    pub capacity: usize,
    pub cursor: usize,
}

impl InstancePool {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance pool"),
            size: (capacity * Instance::STRIDE) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            capacity: capacity.max(1),
            cursor: 0,
        }
    }

    pub fn upload(&mut self, instances: &[Instance], context: &RenderContext) -> usize {
        let size = instances.len();
        if self.cursor + size > self.capacity {
            self.cursor = 0;
        }

        let offset = (self.cursor * Instance::STRIDE) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::cast_slice(instances));

        let start_offset = self.cursor;
        self.cursor = start_offset + size % self.capacity;

        start_offset
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}
