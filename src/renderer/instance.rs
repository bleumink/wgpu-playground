use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::renderer::{context::RenderContext, scene::SceneGraph, vertex::Vertex};

// pub trait Instanced {
//     type Instance: Pod + Vertex;

//     fn pipeline_id() -> &'static str;
//     fn instances(scene: &SceneGraph) -> Vec<Self::Instance>;
//     fn draw();
// }

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct DrawCommand {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: u32,
    pub first_instance: u32,
}

impl DrawCommand {
    pub const STRIDE: usize = std::mem::size_of::<Self>();
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Instance {
    pub position_offset: u32,
    pub surface_offset: u32,
    pub uv_offsets: [u32; 4],
    pub transform_index: u32,
    pub normal_index: u32,
    pub material_index: u32,      
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
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u32; 2]>() as u64,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u32; 6]>() as u64,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u32; 7]>() as u64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u32; 8]>() as u64,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32,
                },                
            ],
        }
    }
}

pub struct InstancePool {
    pub instance_buffer: wgpu::Buffer,
    pub command_buffer: wgpu::Buffer,
    pub capacity: usize,
    pub cursor: usize,
    pub count: usize,
}

impl InstancePool {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let instance_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance pool"),
            size: (capacity * Instance::STRIDE) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let command_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Command buffer"),
            size: (capacity * DrawCommand::STRIDE) as u64,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            instance_buffer,
            command_buffer,
            capacity: capacity.max(1),
            cursor: 0,
            count: 0,
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
            .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(instances));

        let start_offset = self.cursor;
        self.cursor = start_offset + size % self.capacity;

        start_offset
    }

    pub fn upload_commands(&mut self, instances: &[Instance], commands: &[DrawCommand], context: &RenderContext) {
        context.queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));                
        context.queue.write_buffer(&self.command_buffer, 0, bytemuck::cast_slice(commands));
        
        self.cursor = instances.len();        
        self.count = commands.len();        
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    pub fn instances(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }

    pub fn commands(&self) -> &wgpu::Buffer {
        &self.command_buffer
    }

    pub fn count(&self) -> u32 {
        self.count as u32
    }
}
