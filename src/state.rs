use std::{collections::HashMap, sync::Arc};

use crossbeam::channel::{Receiver, Sender};
use instant::Instant;
use uuid::Uuid;
use winit::{event, event_loop::ActiveEventLoop, window::Window};

#[cfg(target_family = "wasm")]
use crate::renderer::Renderer;

use crate::{
    asset::{AssetLoader, ResourcePath},
    camera::{Camera, CameraController, Projection},
    instance::Instance,
    renderer::{RenderCommand, RenderEvent, RenderId},
    surface::{Surface, SurfaceState},
    ui::Ui,
};

pub type EntityId = Uuid;

const MAT4_SWAP_YZ: glam::Mat4 = glam::Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
]);

#[derive(Debug)]
pub struct Entity {
    position: glam::Vec3,
    rotation: glam::Quat,
    scale: glam::Vec3,
    render_id: RenderId,
    label: Option<String>,
}

impl Entity {
    pub fn new_id() -> EntityId {
        Uuid::new_v4()
    }

    pub fn new(
        render_id: RenderId,
        position: glam::Vec3,
        rotation: glam::Quat,
        scale: glam::Vec3,
        label: Option<String>,
    ) -> Self {
        Self {
            position,
            rotation,
            scale,
            render_id,
            label,
        }
    }

    pub fn to_transform(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

pub struct State {
    window: Arc<Window>,
    surface: Surface,
    ui: Ui,
    camera: Camera,
    camera_controller: CameraController,
    projection: Projection,
    loader: AssetLoader,
    timestamp: Instant,
    is_running: bool,
    entities: HashMap<EntityId, Entity>,
    render_tx: Sender<RenderCommand>,
    result_rx: Receiver<RenderEvent>,
    #[cfg(target_family = "wasm")]
    renderer: Renderer,
}

impl State {
    pub async fn new(
        window: Arc<Window>,
        surface: Surface,
        render_sender: Sender<RenderCommand>,
        error_receiver: Receiver<RenderEvent>,
        #[cfg(target_family = "wasm")] renderer: Renderer,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let camera = Camera::new((0.0, 5.0, 10.0), 45.0_f32.to_radians(), -20.0_f32.to_radians());
        let projection = Projection::new(size.width, size.height, 60.0_f32.to_radians(), 0.1, 500.0);
        let camera_controller = CameraController::new(8.0, 0.004);
        let loader = AssetLoader::new(render_sender.clone());
        let ui = Ui::new(Arc::clone(&window), loader.clone());
        let entities = HashMap::new();

        loader.load(ResourcePath::new("cube.obj").unwrap());
        // loader.load(ResourcePath::new("1612_9070.laz"));

        Ok(Self {
            window,
            surface,
            ui,
            camera,
            camera_controller,
            projection,
            loader,
            entities,
            timestamp: Instant::now(),
            is_running: true,
            render_tx: render_sender,
            result_rx: error_receiver,
            #[cfg(target_family = "wasm")]
            renderer,
        })
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.window.request_redraw();

        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                RenderEvent::FrameComplete => {
                    self.surface.present();
                }
                RenderEvent::ResizeComplete { config, device } => {
                    self.surface.apply_resize(config, device);
                }
                RenderEvent::LoadComplete {
                    render_id,
                    transform,
                    label,
                } => {
                    if label.clone().unwrap() == "cube.obj" {
                        const NUM_INSTANCES_PER_ROW: u32 = 10;
                        const INSTANCE_DISPLACEMENT: glam::Vec3 = glam::Vec3 {
                            x: NUM_INSTANCES_PER_ROW as f32 * 0.5,
                            y: 0.0,
                            z: NUM_INSTANCES_PER_ROW as f32 * 0.5,
                        };

                        const SPACE_BETWEEN: f32 = 3.0;
                        let instances = (0..NUM_INSTANCES_PER_ROW)
                            .flat_map(|z| {
                                (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                                    let x = SPACE_BETWEEN * (x as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);
                                    let z = SPACE_BETWEEN * (z as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);

                                    let position = glam::Vec3::new(x as f32, 0.0, z as f32) - INSTANCE_DISPLACEMENT;
                                    let rotation = if position.length_squared() == 0.0 {
                                        glam::Quat::IDENTITY
                                    } else {
                                        glam::Quat::from_axis_angle(position.normalize(), 45.0_f32.to_radians())
                                    };

                                    Instance::new(position, rotation)
                                })
                            })
                            .collect::<Vec<_>>();

                        for instance in instances {
                            let entity_id = Entity::new_id();
                            let entity = Entity::new(
                                render_id,
                                instance.position,
                                instance.rotation,
                                glam::Vec3::ONE,
                                label.clone(),
                            );

                            let translation = glam::Vec3 {
                                x: 0.0,
                                y: -5.0,
                                z: 0.0,
                            };
                            self.render_tx
                                .send(RenderCommand::SpawnAsset {
                                    entity_id,
                                    render_id,
                                    transform: glam::Mat4::from_translation(translation) * entity.to_transform(),
                                })
                                .unwrap();
                            self.entities.insert(entity_id, entity);
                        }
                    } else {
                        let transform = transform.unwrap_or(glam::Mat4::IDENTITY);
                        let (scale, rotation, position) = transform.to_scale_rotation_translation();

                        let entity_id = Entity::new_id();
                        let entity = Entity::new(render_id, position, rotation, scale, label);

                        self.render_tx
                            .send(RenderCommand::SpawnAsset {
                                entity_id,
                                render_id,
                                transform,
                            })
                            .unwrap();
                        self.entities.insert(entity_id, entity);
                    }
                }
                RenderEvent::Stopped => {
                    event_loop.exit();
                }
            }

            if self.is_running && matches!(self.surface.state(), SurfaceState::Configured) {
                if let Err(error) = self.request_frame() {
                    log::error!("Unable to request frame from renderer: {}", error);
                }
            } else if !self.is_running {
                self.render_tx.send(RenderCommand::Stop).unwrap();
                self.surface.drop();
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn request_frame(&mut self) -> anyhow::Result<()> {
        let delta_time = self.timestamp.elapsed();
        self.timestamp = Instant::now();

        self.camera_controller.update_camera(&mut self.camera, delta_time);
        self.render_tx.send(RenderCommand::UpdateCamera {
            position: self.camera.position(),
            view_projection_matrix: self.projection.matrix() * self.camera.view_matrix(),
        })?;

        match self.surface.acquire() {
            Ok(view) => {
                let ui = self.ui.build(self.surface.config(), delta_time);
                self.render_tx.send(RenderCommand::RenderFrame { view, ui })?;
            }
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                let size = self.window().inner_size();
                self.resize(size.width, size.height);
            }
            Err(error) => {
                log::error!("Unable to render surface: {}", error);
            }
        }

        Ok(())
    }

    #[cfg(target_family = "wasm")]
    pub fn update(&mut self, event_loop: &ActiveEventLoop) {
        let delta_time = self.timestamp.elapsed();
        self.timestamp = Instant::now();

        if !self.is_running {
            event_loop.exit();
        }

        self.window.request_redraw();
        self.camera_controller.update_camera(&mut self.camera, delta_time);

        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                RenderEvent::LoadComplete {
                    render_id,
                    transform,
                    label,
                } => {
                    if label.clone().unwrap() == "cube.obj" {
                        const NUM_INSTANCES_PER_ROW: u32 = 10;
                        const INSTANCE_DISPLACEMENT: glam::Vec3 = glam::Vec3 {
                            x: NUM_INSTANCES_PER_ROW as f32 * 0.5,
                            y: 0.0,
                            z: NUM_INSTANCES_PER_ROW as f32 * 0.5,
                        };

                        const SPACE_BETWEEN: f32 = 3.0;
                        let instances = (0..NUM_INSTANCES_PER_ROW)
                            .flat_map(|z| {
                                (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                                    let x = SPACE_BETWEEN * (x as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);
                                    let z = SPACE_BETWEEN * (z as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);

                                    let position = glam::Vec3::new(x as f32, 0.0, z as f32) - INSTANCE_DISPLACEMENT;
                                    let rotation = if position.length_squared() == 0.0 {
                                        glam::Quat::IDENTITY
                                    } else {
                                        glam::Quat::from_axis_angle(position.normalize(), 45.0_f32.to_radians())
                                    };

                                    Instance::new(position, rotation)
                                })
                            })
                            .collect::<Vec<_>>();

                        for instance in instances {
                            let entity_id = Entity::new_id();
                            let entity = Entity::new(
                                render_id,
                                instance.position,
                                instance.rotation,
                                glam::Vec3::ONE,
                                label.clone(),
                            );
                            let translation = glam::Vec3 {
                                x: 0.0,
                                y: -5.0,
                                z: 0.0,
                            };
                            self.render_tx
                                .send(RenderCommand::SpawnAsset {
                                    entity_id,
                                    render_id,
                                    transform: glam::Mat4::from_translation(translation) * entity.to_transform(),
                                })
                                .unwrap();
                            self.entities.insert(entity_id, entity);
                        }
                    } else {
                        let transform = transform.unwrap_or(glam::Mat4::IDENTITY);
                        let (scale, rotation, position) = transform.to_scale_rotation_translation();

                        let entity_id = Entity::new_id();
                        let entity = Entity::new(render_id, position, rotation, scale, label);

                        self.render_tx
                            .send(RenderCommand::SpawnAsset {
                                entity_id,
                                render_id,
                                transform,
                            })
                            .unwrap();
                        self.entities.insert(entity_id, entity);
                    }
                }
                _ => (),
            }
        }

        self.renderer.update_camera(
            self.camera.position(),
            self.projection.matrix() * self.camera.view_matrix(),
        );

        if let Err(error) = self.renderer.run() {
            log::error!("Error handling renderer events: {}", error);
        }

        let view = match self.surface.acquire() {
            Ok(view) => view,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                let size = self.window().inner_size();
                self.resize(size.width, size.height);
                return;
            }
            Err(error) => {
                log::error!("Unable to render surface: {}", error);
                return;
            }
        };

        let ui = self.ui.build(self.surface.config(), delta_time);
        self.renderer.render_frame(view, ui);
        self.surface.present();
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn resize(&mut self, width: u32, height: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        self.projection.resize(width, height);

        let config = self.surface.request_resize(width, height);
        self.render_tx.send(RenderCommand::Resize(config)).unwrap();
    }

    #[cfg(target_family = "wasm")]
    pub fn resize(&mut self, width: u32, height: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        self.projection.resize(width, height);

        let config = self.surface.request_resize(width, height);
        let device = self.renderer.device();
        self.surface.apply_resize(config.clone(), device.clone());

        self.renderer.update_config(config);
    }

    pub fn exit(&mut self) {
        self.is_running = false;        
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    pub fn camera_controller_mut(&mut self) -> &mut CameraController {
        &mut self.camera_controller
    }
}
