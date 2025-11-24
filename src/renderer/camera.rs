use wgpu::util::DeviceExt;

use crate::renderer::context::RenderContext;

pub struct Camera {
    uniform: CameraUniform,
    buffer: wgpu::Buffer,
    // layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(context: &RenderContext) -> Self {
        let uniform = CameraUniform::new();
        let buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // let layout = context
        //     .device
        //     .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        //         label: Some("Camera bind group layout"),
        //         entries: &[wgpu::BindGroupLayoutEntry {
        //             binding: 0,
        //             visibility: wgpu::ShaderStages::VERTEX,
        //             ty: wgpu::BindingType::Buffer {
        //                 ty: wgpu::BufferBindingType::Uniform,
        //                 has_dynamic_offset: false,
        //                 min_binding_size: None,
        //             },
        //             count: None,
        //         }],
        //     });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &context.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform,
            buffer,
            // layout,
            bind_group,
        }
    }

    pub fn update(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4, context: &RenderContext) {
        self.uniform.update(position, view, projection);
        context
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    // pub fn layout(&self) -> &wgpu::BindGroupLayout {
    //     &self.layout
    // }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_position: [f32; 4],
    view_projection: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    inv_projection: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_position: [0.0; 4],
            view_projection: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_view: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_projection: glam::Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4) {
        let view_projection = projection * view;

        self.view_position = position.extend(1.0).to_array();
        self.view_projection = view_projection.to_cols_array_2d();
        self.inv_view = view.transpose().to_cols_array_2d();
        self.inv_projection = projection.inverse().to_cols_array_2d();
    }
}
