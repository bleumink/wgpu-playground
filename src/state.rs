use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use glam::Vec4Swizzles;
use instant::Instant;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    camera::{Camera, CameraController, Projection},
    dialog::open_file_dialog,
    entity::{Entity, EntityId},
    renderer::{AssetLoader, Light, RenderCommand, RenderEvent, RenderId, Renderer, ResourcePath, Ui},
};

pub struct State {
    window: Arc<Window>,
    ui: Ui,
    camera: Camera,
    camera_controller: CameraController,
    projection: Projection,
    loader: AssetLoader,
    timestamp: Instant,
    entities: HashMap<EntityId, Entity>,
    renderer: Renderer,
    event_queue: Vec<RenderEvent>,
    fps: f32,
    light_color: [u8; 3],
    light_intensity: f32,
}

impl State {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let renderer = Renderer::new(Arc::clone(&window)).await;
        let size = window.inner_size();
        let camera = Camera::new((0.0, 5.0, 10.0), 45.0_f32.to_radians(), -20.0_f32.to_radians());
        let projection = Projection::new(size.width, size.height, 60.0_f32.to_radians(), 0.1, 500.0);
        let camera_controller = CameraController::new(8.0, 0.004);
        let loader = AssetLoader::new(renderer.sender());
        let ui = Ui::new(Arc::clone(&window));
        let mut entities = HashMap::new();

        loader.load(ResourcePath::new("cube.obj").unwrap());
        // loader.load(ResourcePath::new("pure-sky.hdr").unwrap());
        // loader.load(ResourcePath::new("1612_9070.laz"));

        let light = Light::Point {
            position: glam::Vec3 {
                x: 2.0,
                y: -3.0,
                z: 2.0,
            },
            color: glam::Vec3 { x: 0.9, y: 0.9, z: 0.6 },
            intensity: 100.0,
        };

        let transform = light.to_transform();
        let entity = Entity::new(transform, Some("light".to_string()));

        renderer.send_command(RenderCommand::SpawnLight {
            entity_id: entity.id(),
            light,
        })?;
        // render_sender.send()?;
        entities.insert(entity.id(), entity);

        let directional = Light::Directional {
            direction: glam::Vec3 {
                x: 0.683,
                y: -0.259,
                z: -0.683,
            },
            color: glam::Vec3 {
                x: 1.0,
                y: 0.956,
                z: 0.897,
            },
            intensity: 1.0,
        };

        let directional_transform = directional.to_transform();
        let directional_entity = Entity::new(directional_transform, Some("dir_light".to_string()));

        renderer.send_command(RenderCommand::SpawnLight {
            entity_id: directional_entity.id(),
            light: directional,
        })?;
        entities.insert(directional_entity.id(), directional_entity);

        Ok(Self {
            window,
            ui,
            camera,
            camera_controller,
            projection,
            loader,
            entities,
            timestamp: Instant::now(),
            renderer,
            event_queue: Vec::new(),
            fps: 0.0,
            light_color: [230, 230, 153],
            light_intensity: 100.0,
        })
    }

    pub fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.window.request_redraw();

        let should_update = self.renderer.poll_events(&mut self.event_queue, event_loop);
        for event in self.event_queue.drain(..) {
            match event {
                RenderEvent::LoadComplete {
                    render_id,
                    transform,
                    label,
                } => {
                    if label.clone().unwrap() == "cube.obj" {
                        for entity in create_instances(label) {
                            self.renderer
                                .send_command(RenderCommand::SpawnAsset {
                                    entity_id: entity.id(),
                                    render_id,
                                    transform: entity.transform(),
                                })
                                .unwrap();
                            self.entities.insert(entity.id(), entity);
                        }
                    } else {
                        let transform = transform.unwrap_or(glam::Mat4::IDENTITY);
                        let entity = Entity::new(transform, label);

                        self.renderer
                            .send_command(RenderCommand::SpawnAsset {
                                entity_id: entity.id(),
                                render_id,
                                transform,
                            })
                            .unwrap();
                        self.entities.insert(entity.id(), entity);
                    }
                }
                _ => (),
            }
        }

        if should_update {
            let timestep = self.timestamp.elapsed();
            self.timestamp = Instant::now();
            let average_fps = self.update_fps(timestep).round();

            // Debug
            let light = self
                .entities
                .values_mut()
                .find(|entity| entity.label().as_ref().unwrap() == "light")
                .unwrap();

            let position = light.transform().w_axis.truncate();
            let rotation = glam::Quat::from_rotation_y(10.0_f32.to_radians() * timestep.as_secs_f32());
            let new_position = rotation * position;
            let transform = glam::Mat4::from_translation(new_position);
            light.set_transform(transform);

            self.renderer
                .send_command(RenderCommand::UpdateTransform {
                    entity_id: light.id(),
                    transform,
                })
                .unwrap();

            // end debug

            // UI
            let ctx = self.ui.begin_frame();

            // egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            //     ui.horizontal(|ui| {
            //         if ui.button("File").clicked() {
            //             log::info!("file?");
            //         }
            //         ui.label("WGPU Speeltuin");
            //     });
            // });

            // egui::SidePanel::left("side_panel")
            //     .resizable(true)
            //     .show(ctx, |ui| {
            //         ui.heading("Sidebar");
            //         ui.separator();
            // if ui.button("Load Asset").clicked() {
            //     open_file_dialog(self.loader.clone());
            // }
            //         ui.label("Status: Ready");
            //     });

            // egui::CentralPanel::default()
            //     .show(ctx, |ui| {
            //         ui.heading("Main content");
            //         ui.label("Put your scene or widgets here!");
            //     });

            egui::Window::new("Debug")
                .resizable(true)
                .movable(true)
                .show(ctx, |ui| {
                    ui.label(format!("FPS: {}", average_fps));
                    ui.add_space(10.0);
                    if ui.button("Load Asset").clicked() {
                        open_file_dialog(self.loader.clone());
                    }
                    ui.add_space(10.0);

                    ui.label("Light color");
                    if ui.color_edit_button_srgb(&mut self.light_color).changed() {
                        self.renderer
                            .send_command(RenderCommand::UpdateLight {
                                entity_id: light.id(),
                                kind: 1,
                                color: glam::Vec3::from_array(self.light_color.map(|u| u as f32 / 255.0)),
                                intensity: self.light_intensity,
                                cutoff: 0.0,
                            })
                            .unwrap();
                    }
                    ui.label("Intensity");
                    if ui
                        .add(egui::Slider::new(&mut self.light_intensity, 0.0..=255.0))
                        .changed()
                    {
                        self.renderer
                            .send_command(RenderCommand::UpdateLight {
                                entity_id: light.id(),
                                kind: 1,
                                color: glam::Vec3::from_array(self.light_color.map(|u| u as f32 / 255.0)),
                                intensity: self.light_intensity,
                                cutoff: 0.0,
                            })
                            .unwrap();
                    }
                });
            // End UI

            let ui_data = self.ui.end_frame();

            self.camera_controller.update_camera(&mut self.camera, timestep);
            self.renderer.update_camera(
                self.camera.position(),
                self.camera.view_matrix(),
                self.projection.matrix(),
            );

            self.renderer.request_frame(&self.window, ui_data);
        }
    }

    pub fn update_fps(&mut self, timestep: Duration) -> f32 {
        let current = 1.0 / timestep.as_secs_f32();
        self.fps = self.fps * 0.9 + current * (1.0 - 0.9);
        self.fps
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        self.projection.resize(width, height);
        self.renderer.resize(width, height);
        self.ui.drop_frame();
    }

    pub fn exit(&mut self) {
        self.renderer.exit();
    }

    pub fn window(&self) -> &Window {
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

fn create_instances(label: Option<String>) -> Vec<Entity> {
    #[derive(Clone)]
    pub struct DemoInstance {
        pub position: glam::Vec3,
        pub rotation: glam::Quat,
    }

    impl DemoInstance {
        pub fn new(position: glam::Vec3, rotation: glam::Quat) -> Self {
            Self { position, rotation }
        }

        pub fn to_mat4(&self) -> glam::Mat4 {
            glam::Mat4::from_rotation_translation(self.rotation, self.position)
        }
    }
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

                DemoInstance::new(position, rotation)
            })
        })
        .collect::<Vec<_>>();

    instances
        .iter()
        .map(|instance| {
            let mut entity = Entity::new(
                glam::Mat4::from_rotation_translation(instance.rotation, instance.position),
                label.clone(),
            );

            let translation = glam::Vec3 {
                x: 0.0,
                y: -5.0,
                z: 0.0,
            };

            entity.translate(translation);
            entity
        })
        .collect()
}
