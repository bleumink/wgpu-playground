use std::{collections::HashMap, sync::Arc};

use crossbeam::channel::{Receiver, Sender};
use instant::Instant;
use uuid::Uuid;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    asset::{AssetLoader, LoadOptions, ResourcePath},
    camera::{Camera, CameraController, Projection},
    instance::Instance,
    renderer::{RenderEvent, RenderResult},
};

pub struct State {
    window: Arc<Window>,
    camera: Camera,
    camera_controller: CameraController,
    projection: Projection,
    loader: AssetLoader,
    timestamp: Instant,
    scene_map: HashMap<Uuid, Option<String>>,
    render_tx: Sender<RenderEvent>,
    result_rx: Receiver<Result<RenderResult, wgpu::SurfaceError>>,
}

impl State {
    pub async fn new(
        window: Arc<Window>,
        render_sender: Sender<RenderEvent>,
        error_receiver: Receiver<Result<RenderResult, wgpu::SurfaceError>>,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let camera = Camera::new((0.0, 5.0, 10.0), 45.0_f32.to_radians(), -20.0_f32.to_radians());
        let projection = Projection::new(size.width, size.height, 45.0_f32.to_radians(), 0.1, 500.0);
        let camera_controller = CameraController::new(8.0, 0.4);
        let loader = AssetLoader::new(render_sender.clone());
        let scene_map = HashMap::new();

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

        loader.load(
            ResourcePath::new("cube.obj"),
            Some(
                [
                    LoadOptions::Instanced(instances),
                    // LoadOptions::Transform(glam::Vec3::new(20.0, 0.0, 0.0)),
                ]
                .to_vec(),
            ),
        );
        loader.load(ResourcePath::new("1612_9070.laz"), None);

        Ok(Self {
            window,
            camera,
            camera_controller,
            projection,
            loader,
            scene_map,
            timestamp: Instant::now(),
            render_tx: render_sender,
            result_rx: error_receiver,
        })
    }

    fn receive_render_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                Ok(RenderResult::Ok) => (),
                Ok(RenderResult::LoadComplete(uuid, label)) => {
                    log::info!("Added object {} // {}", uuid, label.clone().unwrap());
                    self.scene_map.insert(uuid, label);                    
                }
                Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                    let size = self.window().inner_size();
                    self.resize(size.width, size.height);
                }
                Err(error) => {
                    log::error!("Unable to render surface {}", error);
                }
            }
        }
    }

    pub fn update(&mut self) {
        self.receive_render_results();

        let delta_time = self.timestamp.elapsed();
        self.timestamp = Instant::now();

        self.camera_controller.update_camera(&mut self.camera, delta_time);
        let position = self.camera.position();
        let view_projection_matrix = self.projection.matrix() * self.camera.view_matrix();

        self.render_tx
            .send(RenderEvent::CameraUpdate {
                position,
                view_projection_matrix,
            })
            .unwrap();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        self.projection.resize(width, height);
        self.render_tx.send(RenderEvent::Resize { width, height }).unwrap();
    }

    pub fn exit(&self, event_loop: &ActiveEventLoop) {
        self.render_tx.send(RenderEvent::Stop).unwrap();
        event_loop.exit();
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn camera_controller_mut(&mut self) -> &mut CameraController {
        &mut self.camera_controller
    }
}
