use std::{collections::HashMap, f32::DIGITS};

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::{context::RenderContext, entity::EntityId, scene::EntityTransformData, transform::TransformUniform};

pub struct LightId(pub usize);

#[derive(Clone, Debug)]
pub enum Light {
    Directional {
        direction: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
    },
    Point {
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
    },
    Spot {
        position: glam::Vec3,
        direction: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        cutoff: f32,
    },
}

impl Light {
    pub fn to_light_uniform(&self) -> LightUniform {
        match self {
            Self::Directional {
                direction,
                color,
                intensity,
            } => LightUniform {
                position: [0.0, 0.0, 0.0],
                direction: direction.to_array(),
                color: color.to_array(),
                kind: 0,
                intensity: *intensity,
                cutoff: 0.0,
            },
            Self::Point {
                position,
                color,
                intensity,
            } => LightUniform {
                position: position.to_array(),
                direction: [0.0; 3],
                color: color.to_array(),
                kind: 1,
                intensity: *intensity,
                cutoff: 0.0,
            },
            Self::Spot {
                position,
                direction,
                color,
                intensity,
                cutoff,
            } => LightUniform {
                position: position.to_array(),
                direction: direction.to_array(),
                color: color.to_array(),
                kind: 2,
                intensity: *intensity,
                cutoff: *cutoff,
            },
        }
    }

    pub fn to_transform(&self) -> glam::Mat4 {
        fn look_dir(position: glam::Vec3, direction: glam::Vec3) -> glam::Mat4 {
            let dir = direction.normalize();
            let up = if dir.abs_diff_eq(glam::Vec3::Y, 1e-3) {
                glam::Vec3::Z
            } else {
                glam::Vec3::Y
            };

            let right = dir.cross(up).normalize();
            let up = right.cross(dir).normalize();

            Mat4::from_cols(
                right.extend(0.0),
                up.extend(0.0),
                (-dir).extend(0.0),
                position.extend(1.0),
            )
        }

        match self {
            Self::Directional { direction, .. } => look_dir(glam::Vec3::ZERO, *direction),
            Self::Point { position, .. } => glam::Mat4::from_translation(*position),
            Self::Spot {
                position, direction, ..
            } => look_dir(*position, *direction),
        }
    }

    pub fn to_transform_uniform(&self) -> TransformUniform {
        TransformUniform::new(self.to_transform())
    }

    pub fn to_parts(self) -> (LightUniform, TransformUniform) {
        (self.to_light_uniform(), self.to_transform_uniform())
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LightUniform {
    position: [f32; 3],
    kind: u32,
    direction: [f32; 3],
    cutoff: f32,
    color: [f32; 3],
    intensity: f32,
}

pub struct LightBuffer {
    capacity: usize,
    lights: Vec<LightUniform>,
    instances: Vec<EntityTransformData>,
    buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
}

impl LightBuffer {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Light bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let buffer = Self::create_buffer::<LightUniform>(capacity, context);
        let index_buffer = Self::create_buffer::<u32>(capacity, context);
        let bind_group = Self::create_bind_group(&buffer, &index_buffer, &layout, context);

        Self {
            capacity,
            lights: Vec::new(),
            instances: Vec::new(),
            buffer,
            index_buffer,
            bind_group,
            layout,
        }
    }

    pub fn add(&mut self, entity_id: EntityId, uniform: LightUniform, transform_index: usize, context: &RenderContext) {
        let entity_data = EntityTransformData(entity_id, transform_index);
        self.instances.push(entity_data);

        let id = self.lights.len();
        self.lights.push(uniform);
        self.update_buffer(context);
    }

    pub fn iter_indices(&self) -> impl Iterator<Item = u32> {
        self.instances.iter().map(|transform_data| transform_data.1 as u32)
    }

    pub fn write(&mut self, id: LightId, light: Light, context: &RenderContext) {
        let index = id.0;
        if index >= self.lights.len() {
            self.lights.resize(index + 1, LightUniform::zeroed());
        }

        let uniform = light.to_light_uniform();
        self.lights[index] = uniform;

        let offset = (index * std::mem::size_of::<LightUniform>()) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&uniform));
    }

    fn update_buffer(&mut self, context: &RenderContext) {
        if self.lights.len() > self.capacity {
            self.capacity = self.lights.len() * 2;
            self.buffer = Self::create_buffer::<LightUniform>(self.capacity, context);
            self.bind_group = Self::create_bind_group(&self.buffer, &self.index_buffer, &self.layout, context)
        }

        context
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.lights));
    }

    fn create_buffer<T>(capacity: usize, context: &RenderContext) -> wgpu::Buffer {
        context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light storage buffer"),
            size: (capacity * std::mem::size_of::<T>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_bind_group(
        buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        layout: &wgpu::BindGroupLayout,
        context: &RenderContext,
    ) -> wgpu::BindGroup {
        context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: index_buffer.as_entire_binding(),
                },
            ],
        })
    }

    pub fn lights(&self) -> &[LightUniform] {
        &self.lights
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }
}
