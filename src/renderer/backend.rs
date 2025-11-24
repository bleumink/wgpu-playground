use crossbeam::channel::{Receiver, Sender};
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::renderer::{
    RenderCommand, RenderEvent,
    core::RenderCore,
    surface::{Surface, SurfaceState},
    ui::UiData,
};

pub trait RenderBackend {
    fn send_command(&self, command: RenderCommand);
    fn update_camera(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4);
    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop);
    fn resize(&mut self, width: u32, height: u32);
    fn request_frame(&mut self, window: &Window, ui: Option<UiData>);
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

    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                RenderEvent::FrameComplete => {
                    self.surface.present();
                }
                RenderEvent::ResizeComplete { config, device } => {
                    self.surface.apply_resize(config, device);
                }
                RenderEvent::LoadComplete { .. } => {
                    queue.push(event);
                }
                RenderEvent::Stopped => {
                    if let Some(handle) = self.handle.take() {
                        match handle.join() {
                            Ok(_) => self.surface.drop(),
                            Err(error) => log::warn!("Error while terminating renderer {:?}", error),
                        }

                        event_loop.exit();
                    }
                }
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let config = self.surface.request_resize(width, height);
        self.render_tx.send(RenderCommand::Resize(config)).unwrap();
    }

    fn request_frame(&mut self, window: &Window, ui: Option<UiData>) {
        if self.is_running {
            match self.surface.acquire() {
                Ok(view) => {
                    self.render_tx.send(RenderCommand::RenderFrame { view, ui }).unwrap();
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

    fn update_camera(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4) {
        self.render_tx
            .send(RenderCommand::UpdateCamera {
                position,
                view,
                projection,
            })
            .unwrap();
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

    fn poll_events(&mut self, queue: &mut Vec<RenderEvent>, event_loop: &ActiveEventLoop) {
        if !self.is_running {
            event_loop.exit();
        }

        queue.extend(self.event_rx.try_iter());
    }

    fn resize(&mut self, width: u32, height: u32) {
        let config = self.surface.request_resize(width, height);
        let device = self.core.device();
        self.surface.apply_resize(config.clone(), device.clone());
        self.core.update_config(config);
    }

    fn request_frame(&mut self, window: &Window, ui: Option<UiData>) {
        if let Err(error) = self.core.run_once() {
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

        self.core.render_frame(view, ui);
        self.surface.present();
    }

    fn update_camera(&mut self, position: glam::Vec3, view: glam::Mat4, projection: glam::Mat4) {
        self.core.update_camera(position, view, projection);
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
