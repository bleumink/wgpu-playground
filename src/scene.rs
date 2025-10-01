use std::{collections::HashMap, fmt::format, ops::Range};

use uuid::Uuid;
use wgpu::util::DeviceExt;

use crate::{
    context::RenderContext, instance::{Instance, Instances}, model::{DrawModel, Material, Mesh, Model, ModelBuffer}, pointcloud::{DrawPointcloud, Pointcloud, PointcloudBuffer}, renderer::TransformBuffer, texture::Texture
};

pub type Entity = Uuid;

pub fn new_entity() -> Entity {
    Uuid::new_v4()
}

pub enum RenderKind {
    Model(Model),
    Pointcloud(Pointcloud),
}

pub struct Renderable {
    pub kind: RenderKind,
    pub transform_index: usize,
    pub instances: Option<Instances>,
}

impl Renderable {
    pub fn set_instanced(&mut self, instances: &[Instance], context: &RenderContext) {
        self.instances = Some(Instances::new(instances, context))
    }

    // fn get_instances(&self) -> Range<u32> {
    //     if let Some(instances) = &self.instances {
    //         0..instances.data.len() as u32
    //     } else {
    //         0..1
    //     }
    // }    

    pub fn update_transform(&self, transform: glam::Mat4, transform_buffer: &TransformBuffer, context: &RenderContext) {
        transform_buffer.write(self.transform_index, transform, context);
    }
}

pub struct Scene {
    pub transforms: HashMap<Entity, usize>,
    pub renderables: HashMap<Entity, Renderable>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            transforms: HashMap::new(),
            renderables: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, transform_index: usize, renderable: Renderable) -> Entity {
        let id = new_entity();
        self.transforms.insert(id, transform_index);
        self.renderables.insert(id, renderable);
        id
    }

    pub fn remove_entity(&mut self, id: &Entity) {
        self.transforms.remove(id);
        self.renderables.remove(id);
    }

    pub fn iter_models(&self) -> impl Iterator<Item = (&Model, Option<Instances>)> {
        self.renderables
            .iter()
            .filter_map(|(_, renderable)| {
                let instances = renderable.instances;
                if let RenderKind::Model(model) = &renderable.kind {
                    Some((model, renderable.instances))
                } else {
                    None
                }
            })
    }

    pub fn iter_pointclouds(&self) -> impl Iterator<Item = (&Pointcloud, Option<Instances>)> {
        self.renderables
            .iter()
            .filter_map(|(_, renderable)| {
                let instances = renderable.get_instances();
                if let RenderKind::Pointcloud(pointcloud) = &renderable.kind {
                    Some((pointcloud, instances))
                } else {
                    None
                }
            })
    }

    pub fn renderables(&self) -> &HashMap<Entity, Renderable> {
        &self.renderables           
    }
}

pub trait DrawScene<'a> {
    // fn draw_scene(&mut self, scene: &'a Scene, camera_bind_group: &'a wgpu::BindGroup, transform_bind_group: &'a wgpu::BindGroup);
    fn draw_models(
        &mut self,
        scene: &'a Scene,
        camera_bind_group: &'a wgpu::BindGroup,
        transform_bind_group: &'a wgpu::BindGroup,
    );
    fn draw_pointclouds(
        &mut self,
        scene: &'a Scene,
        camera_bind_group: &'a wgpu::BindGroup,
        transform_bind_group: &'a wgpu::BindGroup,
    );
}

impl<'a, 'b> DrawScene<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    // fn draw_scene(&mut self, scene: &'b Scene, camera_bind_group: &'b wgpu::BindGroup, transform_bind_group: &'b wgpu::BindGroup) {
    //     self.set_bind_group(1, camera_bind_group, &[]);
    //     self.set_bind_group(2, transform_bind_group, &[]);        
    //     for model in scene.iter_models() {
    //         self.draw_model(model);
    //     }

    //     self.set_bind_group(0, camera_bind_group, &[]);
    //     self.set_bind_group(1, transform_bind_group, &[]);
    //     for pointcloud in scene.iter_pointclouds() {
    //         self.draw_pointcloud(pointcloud);
    //     }
    // }

    fn draw_models(
        &mut self,
        scene: &'b Scene,
        camera_bind_group: &'b wgpu::BindGroup,
        transform_bind_group: &'b wgpu::BindGroup,
    ) {
        self.set_bind_group(1, camera_bind_group, &[]);
        self.set_bind_group(2, transform_bind_group, &[]);
        for (model, instances) in scene.iter_models() {
            self.draw_model_instanced(model);
        }
    }

    fn draw_pointclouds(
        &mut self,
        scene: &'b Scene,
        camera_bind_group: &'b wgpu::BindGroup,
        transform_bind_group: &'b wgpu::BindGroup,
    ) {
        self.set_bind_group(0, camera_bind_group, &[]);
        self.set_bind_group(1, transform_bind_group, &[]);

        for (pointcloud, instances) in scene.iter_pointclouds() {
            self.draw_pointcloud(pointcloud);
        }
    }
}
