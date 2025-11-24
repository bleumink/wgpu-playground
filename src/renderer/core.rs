use crossbeam::channel::{Receiver, Sender};
use egui_wgpu::Renderer as EguiRenderer;
use uuid::Uuid;

use crate::renderer::{
    RenderCommand, RenderEvent,
    asset::AssetBuffer,
    camera::Camera,
    context::RenderContext,
    environment::{EnvironmentMap, HdrLoader},
    instance::Instance,
    light::{Light, LightUniform},
    mesh::{MeshVertex, Scene, TextureCoordinate},
    pipeline::PipelineCache,
    pointcloud::{PointVertex, Pointcloud},
    scene::{DrawScene, RenderId, SceneGraph},
    texture::Texture,
    transform::TransformUniform,
    ui::UiData,
    vertex::VertexLayoutBuilder,
};

const MAT4_SWAP_YZ: glam::Mat4 = glam::Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
]);

pub struct Frame {
    encoder: wgpu::CommandEncoder,
    view: wgpu::TextureView,
}

impl Frame {
    pub fn new(view: wgpu::TextureView, device: &wgpu::Device) -> Self {
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render encoder"),
        });

        Self { encoder, view }
    }

    pub fn finish(self) -> wgpu::CommandBuffer {
        self.encoder.finish()
    }
}
pub struct RenderCore {
    is_running: bool,
    context: RenderContext,
    camera: Camera,
    scene: SceneGraph,
    pipeline_cache: PipelineCache,
    egui_renderer: EguiRenderer,
    render_rx: Receiver<RenderCommand>,
    result_tx: Sender<RenderEvent>,
}

impl RenderCore {
    pub async fn new(
        context: RenderContext,
        render_receiver: Receiver<RenderCommand>,
        error_sender: Sender<RenderEvent>,
    ) -> anyhow::Result<Self> {
        let camera = Camera::new(&context);
        let egui_renderer = EguiRenderer::new(
            &context.device,
            context.config.format.add_srgb_suffix(),
            Default::default(),
        );
        let scene = SceneGraph::new(&context);
        let mut pipeline_cache = PipelineCache::new();

        let shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../res/shader.wgsl").into()),
        });

        let pointcloud_shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Pointcloud shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../res/pc_shader.wgsl").into()),
        });

        let light_shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Light shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../res/light.wgsl").into()),
        });

        let render_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render pipeline layout"),
            bind_group_layouts: &[
                &context.texture_bind_group_layout,
                &context.camera_bind_group_layout,
                scene.layout(),
            ],
            push_constant_ranges: &[],
        });

        let mesh_vertex_layout = (0..RenderContext::MAX_UV_SETS)
            .fold(VertexLayoutBuilder::new().push::<MeshVertex>(), |builder, _| {
                builder.push::<TextureCoordinate>()
            })
            .push::<Instance>()
            .build();

        let render_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &mesh_vertex_layout,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.hdr.format(),
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
            bind_group_layouts: &[
                &context.texture_bind_group_layout,
                &context.camera_bind_group_layout,
                scene.layout(),
            ],
            push_constant_ranges: &[],
        });

        let pointcloud_vertex_layout = VertexLayoutBuilder::new()
            .push::<PointVertex>()
            .push::<Instance>()
            .build();

        let pointcloud_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pointcloud pipeline"),
            layout: Some(&pointcloud_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &pointcloud_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &pointcloud_vertex_layout,
            },
            fragment: Some(wgpu::FragmentState {
                module: &pointcloud_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.hdr.format(),
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

        let light_debug_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Debug light pipeline layout"),
            bind_group_layouts: &[
                &context.texture_bind_group_layout,
                &context.camera_bind_group_layout,
                scene.layout(),
            ],
            push_constant_ranges: &[],
        });

        let light_debug_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Light debug pipeline"),
            layout: Some(&light_debug_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &light_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &mesh_vertex_layout,
            },
            fragment: Some(wgpu::FragmentState {
                module: &light_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.hdr.format(),
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

        pipeline_cache.insert("mesh", render_pipeline);
        pipeline_cache.insert("pointcloud", pointcloud_pipeline);
        pipeline_cache.insert("light", light_debug_pipeline);

        Ok(Self {
            is_running: true,
            context,
            camera,
            scene,
            pipeline_cache,
            egui_renderer,
            render_rx: render_receiver,
            result_tx: error_sender,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.context.device
    }

    fn load_asset(&mut self, asset: AssetBuffer) -> anyhow::Result<()> {
        match asset {
            AssetBuffer::EnvironmentMap { buffer, label } => {
                let loader = HdrLoader::new(&self.context.device);
                let texture = loader.from_buffer(buffer, 1080, label.as_deref(), &self.context)?;
                let environment_map = EnvironmentMap::new(texture, &self.context);
                self.scene.set_environment_map(environment_map);
            }
            AssetBuffer::Scene(buffer, label) => {
                let scene = Scene::from_buffer(buffer, &self.context, label.clone());
                let material_ids = scene
                    .materials
                    .into_iter()
                    .map(|material| self.scene.add_material(material))
                    .collect::<Vec<_>>();

                for node in scene.nodes {
                    let render_id = self.scene.add_mesh(node.mesh, &material_ids);
                    self.result_tx.send(RenderEvent::LoadComplete {
                        render_id,
                        transform: Some(node.transform),
                        label: label.clone(),
                    })?;
                }
            }
            AssetBuffer::Pointcloud(buffer, label) => {
                let pointcloud = Pointcloud::from_buffer(buffer, &self.context, label.clone());
                let render_id = self.scene.add_pointcloud(pointcloud);

                self.result_tx.send(RenderEvent::LoadComplete {
                    render_id,
                    transform: Some(MAT4_SWAP_YZ),
                    label,
                })?;
            }
        }

        Ok(())
    }

    fn spawn_asset(&mut self, entity_id: Uuid, render_id: RenderId, transform: glam::Mat4) {
        self.scene.add_node(entity_id, render_id, transform, &self.context);
    }

    fn spawn_light(&mut self, entity_id: Uuid, light: Light) {
        self.scene.add_light(entity_id, light, &self.context);
    }

    pub fn render_scene(&self, frame: &mut Frame) {
        let mut render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.context.hdr.view(),
                resolve_target: None,
                depth_slice: None,
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

        render_pass.draw_scene(&self.scene, &self.camera.bind_group(), &self.pipeline_cache);
    }

    pub fn render_ui(&mut self, frame: &mut Frame, ui: UiData) {
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
                depth_slice: None,
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

    pub fn render_hdr(&self, frame: &mut Frame) {
        let mut render_pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HDR render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(self.context.hdr.pipeline());
        render_pass.set_bind_group(0, self.context.hdr.bind_group(), &[]);
        render_pass.draw(0..3, 0..1);
    }

    pub fn render_frame(&mut self, view: wgpu::TextureView, ui: Option<UiData>) {
        self.scene.sync(&self.context);

        let mut frame = Frame::new(view, &self.context.device);
        self.render_scene(&mut frame);
        self.render_hdr(&mut frame);

        if let Some(data) = ui {
            self.render_ui(&mut frame, data);
        }

        self.context.queue.submit(Some(frame.finish()));
    }

    pub fn update_camera(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4) {
        self.camera.update(position, view, projection, &self.context);
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
                view,
                projection,
            } => self.update_camera(position, view, projection),
            RenderCommand::LoadAsset(asset) => self.load_asset(asset)?,
            RenderCommand::SpawnAsset {
                entity_id,
                render_id,
                transform,
            } => self.spawn_asset(entity_id, render_id, transform),
            RenderCommand::SpawnLight { entity_id, light } => self.spawn_light(entity_id, light),
            RenderCommand::Resize(config) => {
                self.context.pending_resize = Some(config.clone());
                self.result_tx.send(RenderEvent::ResizeComplete {
                    config,
                    device: self.context.device.clone(),
                })?;
            }
            RenderCommand::UpdateTransform { entity_id, transform } => {
                let uniform = TransformUniform::new(transform);
                self.scene.transforms.set(&entity_id, uniform, &self.context);
            }
            RenderCommand::UpdateLight {
                entity_id,
                kind,
                color,
                intensity,
                cutoff,
            } => {
                // Need more controls
                let uniform = LightUniform::new(1, color, intensity, cutoff);
                self.scene.lights.set(&entity_id, uniform, &self.context);
            }
            RenderCommand::Stop => {
                self.is_running = false;
            }
        }

        Ok(())
    }

    pub fn run(mut self) -> anyhow::Result<()> {
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

    pub fn run_once(&mut self) -> anyhow::Result<()> {
        while let Ok(command) = self.render_rx.try_recv() {
            self.handle_command(command)?;
        }

        Ok(())
    }
}
