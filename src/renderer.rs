#[cfg(target_family = "wasm")]
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};
use uuid::Uuid;
use wgpu::util::DeviceExt;
use winit::window::Window;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{
    asset::{Asset, LoadOptions},
    context::RenderContext,
    instance::{Instance, RawInstance},
    model::{Model, ModelVertex, TransformUniform},
    pointcloud::{PointVertex, Pointcloud},
    scene::{DrawScene, Renderable, Scene},
    texture::Texture,
    vertex::Vertex,
};

const MAT_SWAP_YZ: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

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

pub struct TransformBuffer {
    capacity: usize,
    next_slot: usize,
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl TransformBuffer {
    pub fn new(capacity: usize, context: &RenderContext) -> Self {
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transform buffer"),
            size: (capacity * std::mem::size_of::<TransformUniform>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Transform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Transform bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            capacity,
            buffer,
            layout,
            bind_group,
            next_slot: 0,
        }
    }

    pub fn write(&self, index: usize, matrix: glam::Mat4, context: &RenderContext) {        
        let offset = (index * std::mem::size_of::<TransformUniform>()) as u64;
        context
            .queue
            .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&matrix.to_cols_array_2d()));               
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn request_slot(&mut self, context: &RenderContext) -> usize {
        let index = self.next_slot;
        self.next_slot += 1;
        
        let matrix = MAT_SWAP_YZ;                
        let offset = index * std::mem::size_of::<TransformUniform>();        
        context
            .queue
            .write_buffer(&self.buffer, offset as u64, bytemuck::bytes_of(&matrix));

        index
    }
}

pub enum RenderEvent {
    CameraUpdate {
        position: glam::Vec3,
        view_projection_matrix: glam::Mat4,
    },
    Resize {
        width: u32,
        height: u32,
    },
    LoadAsset(Asset),
    Translate {
        uuid: Uuid,
        translation: glam::Vec3,
    },
    Stop,
}

pub enum RenderResult {
    Ok,
    LoadComplete(Uuid, Option<String>),
}

pub struct Renderer {
    is_running: bool,
    context: RenderContext,
    camera: Camera,
    scene: Scene,
    transform_buffer: TransformBuffer,
    render_pipeline: wgpu::RenderPipeline,
    pointcloud_pipeline: wgpu::RenderPipeline,
    render_rx: Receiver<RenderEvent>,
    result_tx: Sender<Result<RenderResult, wgpu::SurfaceError>>,
    #[cfg(target_family = "wasm")]
    wasm_closure: Option<Closure<dyn FnMut()>>,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        render_receiver: Receiver<RenderEvent>,
        error_sender: Sender<Result<RenderResult, wgpu::SurfaceError>>,
    ) -> anyhow::Result<Self> {
        let context = RenderContext::new(window).await?;
        let camera = Camera::new(&context);
        let transform_buffer = TransformBuffer::new(10, &context);

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
                buffers: &[ModelVertex::desc(), RawInstance::desc()],
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
            transform_buffer,
            render_pipeline,
            pointcloud_pipeline,
            render_rx: render_receiver,
            result_tx: error_sender,
            #[cfg(target_family = "wasm")]
            wasm_closure: None,
        })
    }

    fn load_asset(&mut self, asset: Asset) {
        match asset {
            Asset::Model(buffer, label, options) => {
                let model = Model::from_buffer(buffer, &self.context, label.clone());
                let transform_index = self.transform_buffer.request_slot(&self.context);
                let mut renderable = Renderable {
                    kind: crate::scene::RenderKind::Model(model),
                    instances: None,
                    transform_index,
                };
                                
                if let Some(options) = options {
                    for option in options {
                        match option {
                            LoadOptions::Instanced(instances) => renderable.set_instanced(&instances, &self.context),
                            LoadOptions::Transform(transform) => renderable.update_transform(transform, &self.transform_buffer, &self.context),
                        }
                    }
                }                

                let entity = self.scene.add_entity(transform_index, renderable);
                self.result_tx
                    .send(Ok(RenderResult::LoadComplete(entity, label)))
                    .unwrap();
            }
            Asset::Pointcloud(buffer, label, options) => {
                let pointcloud = Pointcloud::from_buffer(buffer, &self.context, label.clone());
                let transform_index = self.transform_buffer.request_slot(&self.context);                
                let renderable = Renderable {
                    kind: crate::scene::RenderKind::Pointcloud(pointcloud),
                    instances: None,
                    transform_index,
                };                          

                let entity = self.scene.add_entity(transform_index, renderable);
                self.result_tx
                    .send(Ok(RenderResult::LoadComplete(entity, label)))
                    .unwrap();
            }
        }
    }

    fn render(&mut self) -> Result<RenderResult, wgpu::SurfaceError> {
        self.context.window.request_redraw();

        if !self.context.is_surface_configured {
            return Ok(RenderResult::Ok);
        }

        let output = self.context.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.context.config.format.add_srgb_suffix()),
            ..Default::default()
        });

        let mut encoder = self
            .context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.draw_models(&self.scene, &self.camera.bind_group, &self.transform_buffer.bind_group);

            render_pass.set_pipeline(&self.pointcloud_pipeline);
            render_pass.draw_pointclouds(&self.scene, &self.camera.bind_group, &self.transform_buffer.bind_group);
        }

        self.context.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(RenderResult::Ok)
    }

    pub fn handle_event(&mut self, event: RenderEvent) {
        match event {
            RenderEvent::CameraUpdate {
                position,
                view_projection_matrix,
            } => self.camera.update(position, view_projection_matrix, &self.context),
            RenderEvent::LoadAsset(asset) => self.load_asset(asset),
            RenderEvent::Resize { width, height } => self.context.resize(width, height),
            RenderEvent::Translate { uuid, translation } => (),
            RenderEvent::Stop => {
                self.is_running = false;
            }
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        while self.is_running
            && let Ok(event) = self.render_rx.recv()
        {
            self.handle_event(event);

            while let Ok(event) = self.render_rx.try_recv() {
                self.handle_event(event);
            }

            let render_result = self.render();
            self.result_tx.send(render_result)?;
        }

        Ok(())
    }

    #[cfg(target_family = "wasm")]
    pub fn run_web(self) {
        fn request_animation_frame(f: &Closure<dyn FnMut()>) {
            let window = web_sys::window().unwrap_throw();
            window.request_animation_frame(f.as_ref().unchecked_ref()).unwrap();
        }

        fn render(renderer: &mut Renderer) -> anyhow::Result<()> {
            while let Ok(event) = renderer.render_rx.try_recv() {
                renderer.handle_event(event);
            }

            let render_result = renderer.render();
            renderer.result_tx.send(render_result)?;

            Ok(())
        }

        let renderer = Rc::new(RefCell::new(self));
        let inner = Rc::clone(&renderer);
        let closure = Closure::wrap(Box::new(move || {
            let mut renderer = inner.borrow_mut();
            if let Err(error) = render(&mut renderer) {
                log::error!("Renderer encountered an error: {}", error);
            }

            if renderer.is_running
                && let Some(wasm_closure) = &renderer.wasm_closure
            {
                request_animation_frame(wasm_closure);
            }
        }) as Box<dyn FnMut()>);

        renderer.borrow_mut().wasm_closure = Some(closure);

        let renderer_borrow = renderer.borrow();
        request_animation_frame(renderer_borrow.wasm_closure.as_ref().unwrap());
    }
}
