use std::time::Duration;

use crossbeam::channel::{Receiver, Sender};
use instant::Instant;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    renderer::{
        RenderCommand, RenderEvent,
        core::RenderCore,
        surface::{Surface, SurfaceState},
    },
    state::State,
    ui::{Ui, UiData},
};

pub trait RenderBackend {
    fn send_command(&self, command: RenderCommand);
    fn update_camera(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4);
    fn update_ui(&mut self, ui: &mut Ui, timestep: Duration);
    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) -> bool;
    fn resize(&mut self, width: u32, height: u32);
    fn request_frame(&mut self, window: &Window);
    fn is_configured(&self) -> bool;
    fn exit(&mut self);
}

pub struct NativeBackend {
    surface: Surface,
    render_tx: Sender<RenderCommand>,
    event_rx: Receiver<RenderEvent>,
    handle: Option<std::thread::JoinHandle<()>>,
    is_running: bool,
}

impl RenderBackend for NativeBackend {
    fn send_command(&self, command: RenderCommand) {
        self.render_tx.send(command).ok();
    }

    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) -> bool {
        let mut should_update = false;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                RenderEvent::FrameComplete => {
                    self.surface.present();
                    should_update = true;
                }
                RenderEvent::ResizeComplete { config, device } => {
                    self.surface.apply_resize(config, device);
                    should_update = true;
                }
                RenderEvent::LoadComplete { .. } => {
                    queue.push(event);
                }
                RenderEvent::Stopped => {
                    if let Some(handle) = self.handle.take() {
                        match handle.join() {
                            Ok(_) => self.surface.drop(),
                            Err(error) => log::warn!("Error while terminating renderer {:?}", error)
                        }

                        event_loop.exit();
                    }
                }
            }
        }

        should_update
    }

    fn resize(&mut self, width: u32, height: u32) {
        let config = self.surface.request_resize(width, height);
        self.render_tx.send(RenderCommand::Resize(config)).unwrap();
    }

    fn request_frame(&mut self, window: &Window) {
        if self.is_running {
            match self.surface.acquire() {
                Ok(view) => {
                    self.render_tx.send(RenderCommand::RenderFrame { view }).unwrap();
                }
                Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                    let size = window.inner_size();
                    self.resize(size.width, size.height);
                }
                Err(error) => {
                    log::error!("Unable to render surface: {}", error);
                }
            }
        } else {
            self.render_tx.send(RenderCommand::Stop).unwrap();
        }
    }

    fn update_camera(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4) {
        self.render_tx
            .send(RenderCommand::UpdateCamera {
                position,
                view_projection_matrix,
            })
            .unwrap();
    }

    fn update_ui(&mut self, ui: &mut Ui, timestep: Duration) {
        let data = ui.build(self.surface.config(), timestep);
        self.render_tx.send(RenderCommand::UpdateUi { data }).unwrap();
    }

    fn is_configured(&self) -> bool {
        matches!(self.surface.state(), SurfaceState::Configured)
    }

    fn exit(&mut self) {
        self.is_running = false;
    }
}

impl NativeBackend {
    pub fn new(
        surface: Surface,
        core: RenderCore,
        render_tx: Sender<RenderCommand>,
        event_rx: Receiver<RenderEvent>,
    ) -> Self {
        let join_handle = std::thread::spawn(move || {
            if let Err(error) = core.run() {
                log::error!("Renderer encountered an error: {}", error);
            }
        });

        Self {
            surface,
            handle: Some(join_handle),
            render_tx,
            event_rx,
            is_running: true,
        }
    }
}

pub struct WasmBackend {
    surface: Surface,
    render_tx: Sender<RenderCommand>,
    event_rx: Receiver<RenderEvent>,
    core: RenderCore,
    is_running: bool,
}

impl RenderBackend for WasmBackend {
    fn send_command(&self, command: RenderCommand) {
        self.render_tx.send(command).ok();
    }

    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) -> bool {
        if !self.is_running {
            event_loop.exit();
        }

        queue.extend(self.event_rx.try_iter());
        true
    }

    fn resize(&mut self, width: u32, height: u32) {
        let config = self.surface.request_resize(width, height);
        let device = self.core.device();
        self.surface.apply_resize(config.clone(), device.clone());
        self.core.update_config(config);
    }

    fn request_frame(&mut self, window: &Window) {
        if let Err(error) = self.core.run_wasm() {
            log::error!("Error handling renderer events: {}", error);
        }

        let view = match self.surface.acquire() {
            Ok(view) => view,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                let size = window.inner_size();
                if size.width > 0 && size.height > 0 {
                    self.resize(size.width, size.height);
                }

                return;
            }
            Err(error) => {
                log::error!("Unable to render surface: {}", error);
                return;
            }
        };

        self.core.render_frame(view);
        self.surface.present();
    }

    fn update_camera(&mut self, position: glam::Vec3, view_projection_matrix: glam::Mat4) {
        self.core.update_camera(position, view_projection_matrix);
    }

    fn update_ui(&mut self, ui: &mut Ui, timestep: Duration) {
        let data = ui.build(self.surface.config(), timestep);
        self.core.update_ui(data);
    }

    fn is_configured(&self) -> bool {
        matches!(self.surface.state(), SurfaceState::Configured)
    }

    fn exit(&mut self) {
        self.is_running = false;
    }
}

impl WasmBackend {
    pub fn new(
        surface: Surface,
        core: RenderCore,
        render_tx: Sender<RenderCommand>,
        event_rx: Receiver<RenderEvent>,
    ) -> Self {
        Self {
            surface,
            core,
            render_tx,
            event_rx,
            is_running: true,
        }
    }
}
