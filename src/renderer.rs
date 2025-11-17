use std::{sync::Arc, time::Duration};

use crossbeam::channel::Sender;
use uuid::Uuid;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    renderer::{
        asset::AssetBuffer,
        backend::RenderBackend,
        core::RenderCore,
        surface::Surface,
    },
    ui::{Ui, UiData},
};

pub use {
    asset::{AssetLoader, ResourcePath},
    light::Light,
    scene::RenderId,
};

mod asset;
mod backend;
mod binary;
mod camera;
mod component;
mod context;
mod core;
mod hdr;
mod instance;
mod light;
mod material;
mod mesh;
mod pipeline;
mod pointcloud;
mod scene;
mod surface;
mod texture;
mod transform;
mod vertex;
#[cfg(target_family = "wasm")]
mod worker;

pub enum RenderCommand {
    RenderFrame {
        view: wgpu::TextureView,
    },
    UpdateCamera {
        position: glam::Vec3,
        view_projection_matrix: glam::Mat4,
    },
    UpdateUi {
        data: UiData,
    },
    Resize(wgpu::SurfaceConfiguration),
    LoadAsset(AssetBuffer),
    SpawnAsset {
        entity_id: Uuid,
        render_id: RenderId,
        transform: glam::Mat4,
    },
    SpawnLight {
        entity_id: Uuid,
        light: Light,
    },
    UpdateTransform {
        entity_id: Uuid,
        transform: glam::Mat4,
    },
    Stop,
}

#[derive(Debug)]
pub enum RenderEvent {
    FrameComplete,
    LoadComplete {
        render_id: RenderId,
        transform: Option<glam::Mat4>,
        label: Option<String>,
    },
    ResizeComplete {
        config: wgpu::SurfaceConfiguration,
        device: wgpu::Device,
    },
    Stopped,
}

pub struct Renderer {
    render_tx: Sender<RenderCommand>,
    backend: Box<dyn RenderBackend>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let (render_tx, render_rx) = crossbeam::channel::unbounded();
        let (event_tx, event_rx) = crossbeam::channel::unbounded();

        let (surface, context) = Surface::initialize(Arc::clone(&window))
            .await
            .expect("Unable to initialize surface");

        let core = RenderCore::new(context, render_rx, event_tx)
            .await
            .expect("Unable to create renderer");

        let backend: Box<dyn RenderBackend> = Box::new({
            #[cfg(not(target_family = "wasm"))]
            {
                use crate::renderer::backend::NativeBackend;
                NativeBackend::new(surface, core, render_tx.clone(), event_rx)
            }
            #[cfg(target_family = "wasm")]
            {
                use crate::renderer::backend::WasmBackend;
                WasmBackend::new(surface, core, render_tx.clone(), event_rx)
            }
        });

        Self {
            render_tx,
            backend,
        }
    }

    pub fn request_frame(&mut self, window: &Window) {
        self.backend.request_frame(window);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.backend.resize(width, height);
    }

    pub fn update_ui(&mut self, ui: &mut Ui, timestep: Duration) {        
        self.backend.update_ui(ui, timestep);
    }

    pub fn update_camera(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4) {
        self.backend.update_camera(position, view_projection_matrix);
    }

    pub fn exit(&mut self) {        
        self.backend.exit();        
    }

    pub fn is_ready(&self) -> bool {
        self.backend.is_configured()
    }

    pub fn sender(&self) -> Sender<RenderCommand> {
        self.render_tx.clone()
    }

    pub fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) -> bool {
        self.backend.poll_events(queue, event_loop)
    }

    pub fn send_command(&self, command: RenderCommand) -> anyhow::Result<()> {
        Ok(self.backend.send_command(command))
    }
}
