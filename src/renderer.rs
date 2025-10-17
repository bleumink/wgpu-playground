#[cfg(target_family = "wasm")]
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};
use egui_wgpu::Renderer as EguiRenderer;
use egui_winit::State as EguiState;
use uuid::Uuid;
use wgpu::util::DeviceExt;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{
    asset::Asset,
    context::RenderContext,
    mesh::{Mesh, MeshVertex, TextureCoordinate},
    pointcloud::{PointVertex, Pointcloud},
    scene::{DrawScene, RenderKind, Scene},
    state::EntityId,
    surface::Surface,
    texture::Texture,
    ui::UiData,
    vertex::Vertex,
};

// pub const MAT_SWAP_YZ: [[f32; 4]; 4] = [
//     [1.0, 0.0, 0.0, 0.0],
//     [0.0, 0.0, 1.0, 0.0],
//     [0.0, 1.0, 0.0, 0.0],
//     [0.0, 0.0, 0.0, 1.0],
// ];

pub type RenderId = Uuid;

struct Camera {
    pub uniform: CameraUniform,
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(context: &RenderContext) -> Self {
        let uniform = CameraUniform::new();
        let buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform,
            buffer,
            layout,
            bind_group,
        }
    }

    pub fn update(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4, context: &RenderContext) {
        self.uniform.update(position, view_projection_matrix);
        context
            .queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_position: [f32; 4],
    view_projection: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_position: [0.0; 4],
            view_projection: glam::Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4) {
        self.view_position = position.extend(1.0).to_array();
        self.view_projection = view_projection_matrix.to_cols_array_2d();
    }
}

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

pub struct Frame {
    encoder: wgpu::CommandEncoder,
    view: wgpu::TextureView,
}

impl Frame {
    pub fn new(view: wgpu::TextureView, context: &RenderContext) -> Self {
        let encoder = context.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render encoder"),
        });

        Self { encoder, view }
    }

    pub fn finish(self) -> wgpu::CommandBuffer {
        self.encoder.finish()
    }
}

pub enum RenderCommand {
    RenderFrame {
        view: wgpu::TextureView,
        ui: UiData,
    },
    UpdateCamera {
        position: glam::Vec3,
        view_projection_matrix: glam::Mat4,
    },
    Resize(wgpu::SurfaceConfiguration),
    LoadAsset(Asset),
    SpawnAsset {
        entity_id: EntityId,
        render_id: RenderId,
        transform: glam::Mat4,
    },
    Translate {
        uuid: Uuid,
        translation: glam::Vec3,
    },
    Stop,
}

#[derive(Debug)]
pub enum RenderEvent {
    FrameComplete,
    LoadComplete(RenderId, Option<String>),
    ResizeComplete(wgpu::SurfaceConfiguration, wgpu::Device),
    Stopped,
}

pub struct Renderer {
    is_running: bool,
    context: RenderContext,
    camera: Camera,
    scene: Scene,
    egui_renderer: EguiRenderer,
    transform_buffer: TransformBuffer,
    render_pipeline: wgpu::RenderPipeline,
    pointcloud_pipeline: wgpu::RenderPipeline,
    render_rx: Receiver<RenderCommand>,
    result_tx: Sender<RenderEvent>,
}

impl Renderer {
    pub async fn new(
        context: RenderContext,
        render_receiver: Receiver<RenderCommand>,
        error_sender: Sender<RenderEvent>,
    ) -> anyhow::Result<Self> {
        let camera = Camera::new(&context);
        let egui_renderer = EguiRenderer::new(&context.device, context.config.format.add_srgb_suffix(), None, 1, true);
        let transform_buffer = TransformBuffer::new(128, &context);

        let shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../res/shader.wgsl").into()),
        });

        let pointcloud_shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Pointcloud shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../res/pc_shader.wgsl").into()),
        });

        let render_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render pipeline layout"),
            bind_group_layouts: &[
                &context.texture_bind_group_layout,
                &camera.layout,
                &transform_buffer.layout,
            ],
            push_constant_ranges: &[],
        });

        let render_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[MeshVertex::desc(), TextureCoordinate::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.config.format.add_srgb_suffix(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let pointcloud_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pointcloud pipeline layout"),
            bind_group_layouts: &[&camera.layout, &transform_buffer.layout],
            push_constant_ranges: &[],
        });

        let pointcloud_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pointcloud pipeline"),
            layout: Some(&pointcloud_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &pointcloud_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[PointVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &pointcloud_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.config.format.add_srgb_suffix(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let scene = Scene::new();

        Ok(Self {
            is_running: true,
            context,
            camera,
            scene,
            egui_renderer,
            transform_buffer,
            render_pipeline,
            pointcloud_pipeline,
            render_rx: render_receiver,
            result_tx: error_sender,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.context.device
    }

    fn load_asset(&mut self, asset: Asset) -> anyhow::Result<()> {
        match asset {
            Asset::Model(buffer, label) => {
                let model = Mesh::from_buffer(buffer, &self.context, label.clone());
                let render_id = self.scene.add_group(
                    RenderKind::Model(model),
                    self.render_pipeline.clone(),
                    &self.transform_buffer,
                    &self.context,
                );
                self.result_tx.send(RenderEvent::LoadComplete(render_id, label))?;
            }
            Asset::Pointcloud(buffer, label) => {
                let pointcloud = Pointcloud::from_buffer(buffer, &self.context, label.clone());
                let render_id = self.scene.add_group(
                    RenderKind::Pointcloud(pointcloud),
                    self.pointcloud_pipeline.clone(),
                    &self.transform_buffer,
                    &self.context,
                );
                self.result_tx.send(RenderEvent::LoadComplete(render_id, label))?;
            }
        }

        Ok(())
    }

    fn spawn_asset(&mut self, entity_id: EntityId, render_id: RenderId, transform: glam::Mat4) {
        let transform_index = self.transform_buffer.request_slot(&self.context);
        self.transform_buffer.write(transform_index, transform, &self.context);
        self.scene
            .add_entity(render_id, entity_id, transform_index, &self.context);
    }

    pub fn render_scene(&mut self, frame: &mut Frame) {
        let mut render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                resolve_target: None,
                // depth_slice: None, Reactivate with 26.0
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.context.depth_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.draw_scene(&self.scene, &self.camera.bind_group);
    }

    pub fn render_ui(&mut self, ui: UiData, frame: &mut Frame) {
        for (id, image_delta) in ui.textures_delta.set.iter() {
            self.egui_renderer
                .update_texture(&self.context.device, &self.context.queue, *id, image_delta);
        }

        self.egui_renderer.update_buffers(
            &self.context.device,
            &self.context.queue,
            &mut frame.encoder,
            &ui.paint_jobs,
            &ui.screen_descriptor,
        );

        let render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Egui render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        self.egui_renderer.render(
            &mut render_pass.forget_lifetime(),
            &ui.paint_jobs,
            &ui.screen_descriptor,
        );
    }

    pub fn render_frame(&mut self, view: wgpu::TextureView, ui: UiData) {
        let mut frame = Frame::new(view, &self.context);
        self.render_scene(&mut frame);
        self.render_ui(ui, &mut frame);
        self.context.queue.submit(Some(frame.finish()));
    }

    pub fn update_camera(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4) {
        self.camera.update(position, view_projection_matrix, &self.context);
    }

    pub fn update_config(&mut self, config: wgpu::SurfaceConfiguration) {
        self.context.resize(config);
    }

    pub fn handle_command(&mut self, command: RenderCommand) -> anyhow::Result<()> {
        match command {
            RenderCommand::RenderFrame { view, ui } => {
                self.render_frame(view, ui);
                self.result_tx.send(RenderEvent::FrameComplete)?;

                if let Some(config) = self.context.pending_resize.take() {
                    self.context.resize(config);
                }
            }
            RenderCommand::UpdateCamera {
                position,
                view_projection_matrix,
            } => self.update_camera(position, view_projection_matrix),
            RenderCommand::LoadAsset(asset) => self.load_asset(asset)?,
            RenderCommand::SpawnAsset {
                entity_id,
                render_id,
                transform,
            } => self.spawn_asset(entity_id, render_id, transform),
            RenderCommand::Resize(config) => {
                self.context.pending_resize = Some(config.clone());
                self.result_tx
                    .send(RenderEvent::ResizeComplete(config, self.context.device.clone()))?;
            }
            RenderCommand::Translate { uuid, translation } => (),
            RenderCommand::Stop => {
                self.is_running = false;
            }
        }

        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn run(&mut self) -> anyhow::Result<()> {
        struct Inbox {
            camera: Option<RenderCommand>,
            resize: Option<RenderCommand>,
            frame: Option<RenderCommand>,
        }

        impl Default for Inbox {
            fn default() -> Self {
                Self {
                    camera: None,
                    resize: None,
                    frame: None,
                }
            }
        }

        impl Inbox {
            fn receive(&mut self, command: RenderCommand) -> Option<RenderCommand> {
                match command {
                    RenderCommand::UpdateCamera { .. } => self.camera = Some(command),
                    RenderCommand::Resize(_) => self.resize = Some(command),
                    RenderCommand::RenderFrame { .. } => self.frame = Some(command),
                    other => return Some(other),
                }

                None
            }

            fn take_ready(&mut self) -> impl Iterator<Item = RenderCommand> {
                let resize = self.resize.take();
                let camera = self.camera.take();
                let frame = self.frame.take();

                [resize, camera, frame].into_iter().flatten()
            }
        }

        let mut inbox = Inbox::default();
        while self.is_running {
            if let Ok(command) = self.render_rx.recv() {
                if let Some(command) = inbox.receive(command) {
                    self.handle_command(command)?;
                }
            }

            while let Ok(command) = self.render_rx.try_recv() {
                if let Some(command) = inbox.receive(command) {
                    self.handle_command(command)?;
                }
            }

            for command in inbox.take_ready() {
                self.handle_command(command)?;
            }
        }

        self.result_tx.send(RenderEvent::Stopped)?;
        Ok(())
    }

    #[cfg(target_family = "wasm")]
    pub fn run(&mut self) -> anyhow::Result<()> {
        while let Ok(command) = self.render_rx.try_recv() {
            self.handle_command(command)?;
        }

        Ok(())
    }
}
