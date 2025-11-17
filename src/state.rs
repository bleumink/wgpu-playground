use std::{collections::HashMap, sync::Arc, time::Duration};

use instant::Instant;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    camera::{Camera, CameraController, Projection},
    entity::{Entity, EntityId},
    renderer::{AssetLoader, Light, RenderCommand, RenderEvent, RenderId, Renderer, ResourcePath},
    ui::Ui,
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
}

impl State {
    pub async fn new(
        window: Arc<Window>,
    ) -> anyhow::Result<Self> {
        let renderer = Renderer::new(Arc::clone(&window)).await;
        let size = window.inner_size();
        let camera = Camera::new((0.0, 5.0, 10.0), 45.0_f32.to_radians(), -20.0_f32.to_radians());
        let projection = Projection::new(size.width, size.height, 60.0_f32.to_radians(), 0.1, 500.0);
        let camera_controller = CameraController::new(8.0, 0.004);
        let loader = AssetLoader::new(renderer.sender());
        let ui = Ui::new(Arc::clone(&window), loader.clone());
        let mut entities = HashMap::new();

        loader.load(ResourcePath::new("cube.obj").unwrap());
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

        if should_update && self.renderer.is_ready() {
            let timestep = self.timestamp.elapsed();
            self.timestamp = Instant::now();

            self.update_camera(timestep);
            self.update_ui(timestep);

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

            self.renderer.request_frame(&self.window);
        }
    }

    fn update_camera(&mut self, timestep: Duration) {
        self.camera_controller.update_camera(&mut self.camera, timestep);
        self.renderer.update_camera(
            self.camera.position(),
            self.projection.matrix() * self.camera.view_matrix(),
        );
    }

    fn update_ui(&mut self, timestep: Duration) {
        self.renderer.update_ui(&mut self.ui, timestep);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        self.projection.resize(width, height);
        self.renderer.resize(width, height);
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

    instances.iter().map(|instance| {
        let mut entity = Entity::new(
            glam::Mat4::from_rotation_translation(instance.rotation, instance.position), 
            label.clone()
        );

        let translation = glam::Vec3 {
            x: 0.0,
            y: -5.0,
            z: 0.0,
        };    

        entity.translate(translation);
        entity        
    }).collect()
}