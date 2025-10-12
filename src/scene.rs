use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    context::RenderContext,
    model::{DrawModel, Model},
    pointcloud::{DrawPointcloud, Pointcloud},
    renderer::{RenderId, TransformBuffer},
    state::EntityId,
};

pub enum RenderKind {
    Model(Model),
    Pointcloud(Pointcloud),
}

pub struct EntityTransformData(EntityId, usize);

pub struct RenderGroup {
    kind: RenderKind,
    instances: Vec<EntityTransformData>,
    index_buffer: wgpu::Buffer,
    buffer_capacity: usize,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl RenderGroup {
    pub fn new(
        kind: RenderKind,
        capacity: usize,
        pipeline: wgpu::RenderPipeline,
        transform_buffer: &TransformBuffer,
        context: &RenderContext,
    ) -> Self {
        let index_buffer = Self::create_buffer(capacity, context);
        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Transform & index bind group"),
            layout: transform_buffer.layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: transform_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: index_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            kind,
            instances: Vec::new(),
            index_buffer,
            buffer_capacity: capacity.max(1),
            pipeline,
            bind_group,
        }
    }

    fn create_buffer(capacity: usize, context: &RenderContext) -> wgpu::Buffer {
        context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance index buffer"),
            size: (capacity * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn update_buffer(&mut self, context: &RenderContext) {
        let indices = self.iter_indices().collect::<Vec<_>>();

        if indices.len() > self.buffer_capacity {
            self.buffer_capacity = indices.len() * 2;
            self.index_buffer = Self::create_buffer(self.buffer_capacity, context);
        }

        context
            .queue
            .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));
    }

    pub fn add_instance(&mut self, entity_id: EntityId, transform_index: usize, context: &RenderContext) {
        let entity_data = EntityTransformData(entity_id, transform_index);
        self.instances.push(entity_data);
        self.update_buffer(context);
    }

    pub fn iter_indices(&self) -> impl Iterator<Item = u32> {
        self.instances.iter().map(|transform_data| transform_data.1 as u32)
    }
}

pub struct Scene {
    pub groups: HashMap<RenderId, RenderGroup>,
}

impl Scene {
    pub fn new() -> Self {
        Self { groups: HashMap::new() }
    }

    pub fn add_group(
        &mut self,
        kind: RenderKind,
        pipeline: wgpu::RenderPipeline,
        transform_buffer: &TransformBuffer,
        context: &RenderContext,
    ) -> RenderId {
        let id = Uuid::new_v4();
        let bucket = RenderGroup::new(kind, 64, pipeline, transform_buffer, context);
        self.groups.insert(id, bucket);
        id
    }

    pub fn add_entity(
        &mut self,
        render_id: RenderId,
        entity_id: EntityId,
        transform_index: usize,
        context: &RenderContext,
    ) {
        let bucket = self.groups.get_mut(&render_id).unwrap();
        bucket.add_instance(entity_id, transform_index, context);
    }

    // pub fn remove_entity(&mut self, id: &EntityId) {
    //     // self.transforms.remove(id);
    //     self.renderables.remove(id);
    // }

    pub fn iter_models(&self) -> impl Iterator<Item = (&Model, &RenderGroup)> {
        self.groups.iter().filter_map(|(_, bucket)| {
            if let RenderKind::Model(model) = &bucket.kind {
                Some((model, bucket))
            } else {
                None
            }
        })
    }

    pub fn iter_pointclouds(&self) -> impl Iterator<Item = (&Pointcloud, &RenderGroup)> {
        self.groups.iter().filter_map(|(_, bucket)| {
            if let RenderKind::Pointcloud(pointcloud) = &bucket.kind {
                Some((pointcloud, bucket))
            } else {
                None
            }
        })
    }
}

pub trait DrawScene<'a> {
    fn draw_scene(&mut self, scene: &'a Scene, camera_bind_group: &'a wgpu::BindGroup);
}

impl<'a, 'b> DrawScene<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    fn draw_scene(&mut self, scene: &'b Scene, camera_bind_group: &'b wgpu::BindGroup) {
        self.set_bind_group(1, camera_bind_group, &[]);
        for (model, bucket) in scene.iter_models() {
            self.set_pipeline(&bucket.pipeline);
            self.set_bind_group(2, &bucket.bind_group, &[]);
            self.draw_model_instanced(model, 0..bucket.instances.len() as u32);
        }

        self.set_bind_group(0, camera_bind_group, &[]);
        for (pointcloud, bucket) in scene.iter_pointclouds() {
            self.set_pipeline(&bucket.pipeline);
            self.set_bind_group(1, &bucket.bind_group, &[]);
            self.draw_pointcloud(pointcloud);
        }
    }
}
