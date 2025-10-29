use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    context::RenderContext,
    material::MaterialInstance,
    mesh::{DrawMesh, Mesh, Scene},
    pointcloud::{DrawPointcloud, Pointcloud},
    renderer::RenderId,
    state::EntityId,
    transform::TransformBuffer,
};

pub enum RenderKind {
    Mesh(Mesh, MaterialsId),
    Pointcloud(Pointcloud),
}

pub struct MaterialData {
    scene_id: SceneId, 
    materials: Vec<MaterialInstance>
}

pub type SceneId = Uuid;

#[derive(Clone)]
pub struct MaterialsId(SceneId, usize);
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
        let bind_group = Self::create_bind_group(transform_buffer, &index_buffer, context);

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

    fn create_bind_group(
        transform_buffer: &TransformBuffer,
        index_buffer: &wgpu::Buffer,
        context: &RenderContext,
    ) -> wgpu::BindGroup {
        context.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        })
    }

    fn update_buffer(&mut self, transform_buffer: &TransformBuffer, context: &RenderContext) {
        let indices = self.iter_indices().collect::<Vec<_>>();

        if indices.len() > self.buffer_capacity {
            self.buffer_capacity = indices.len() * 2;
            self.index_buffer = Self::create_buffer(self.buffer_capacity, context);
            self.bind_group = Self::create_bind_group(transform_buffer, &self.index_buffer, context)
        }

        context
            .queue
            .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));
    }

    pub fn add_instance(
        &mut self,
        entity_id: EntityId,
        transform_index: usize,
        transform_buffer: &TransformBuffer,
        context: &RenderContext,
    ) {
        let entity_data = EntityTransformData(entity_id, transform_index);
        self.instances.push(entity_data);
        self.update_buffer(transform_buffer, context);
    }

    pub fn iter_indices(&self) -> impl Iterator<Item = u32> {
        self.instances.iter().map(|transform_data| transform_data.1 as u32)
    }
}

pub struct SceneGraph {
    pub groups: HashMap<RenderId, RenderGroup>,
    pub transform_buffer: TransformBuffer,
    pub materials: Vec<MaterialData>,
}

impl SceneGraph {
    pub fn new(context: &RenderContext) -> Self {
        Self {
            groups: HashMap::new(),
            transform_buffer: TransformBuffer::new(64, context),
            materials: Vec::new(),
        }
    }

    pub fn transform_layout(&self) -> &wgpu::BindGroupLayout {
        self.transform_buffer.layout()
    }

    pub fn add_materials(&mut self, materials: Vec<MaterialInstance>) -> MaterialsId {
        let scene_id = Uuid::new_v4();
        let index = self.materials.len();
        self.materials.push(MaterialData { 
            scene_id: scene_id.clone(), 
            materials 
        });
        
        MaterialsId(scene_id, index)
    }

    pub fn add_group(&mut self, kind: RenderKind, pipeline: wgpu::RenderPipeline, context: &RenderContext) -> RenderId {
        let id = Uuid::new_v4();
        let bucket = RenderGroup::new(kind, 64, pipeline, &self.transform_buffer, context);
        self.groups.insert(id, bucket);
        id
    }

    pub fn add_entity(
        &mut self,
        render_id: RenderId,
        entity_id: EntityId,
        transform: glam::Mat4,
        context: &RenderContext,
    ) {
        let transform_index = self.transform_buffer.request_slot(context);
        self.transform_buffer.write(transform_index, transform, context);

        let bucket = self.groups.get_mut(&render_id).unwrap();
        bucket.add_instance(entity_id, transform_index, &self.transform_buffer, context);
    }

    pub fn iter_meshes(&self) -> impl Iterator<Item = (&Mesh, &Vec<MaterialInstance>, &RenderGroup)> {
        self.groups.iter().filter_map(|(_, bucket)| {
            if let RenderKind::Mesh(mesh, material_id) = &bucket.kind {
                let materials = &self.materials[material_id.1].materials;
                Some((mesh, materials, bucket))
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
    fn draw_scene(&mut self, scene: &'a SceneGraph, camera_bind_group: &'a wgpu::BindGroup);
}

impl<'a, 'b> DrawScene<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    fn draw_scene(&mut self, scene: &'b SceneGraph, camera_bind_group: &'b wgpu::BindGroup) {
        self.set_bind_group(1, camera_bind_group, &[]);
        for (mesh, materials, bucket) in scene.iter_meshes() {
            self.set_pipeline(&bucket.pipeline);
            self.set_bind_group(2, &bucket.bind_group, &[]);
            self.draw_mesh_instanced(mesh, materials, 0..bucket.instances.len() as u32);
        }

        self.set_bind_group(0, camera_bind_group, &[]);
        for (pointcloud, bucket) in scene.iter_pointclouds() {
            self.set_pipeline(&bucket.pipeline);
            self.set_bind_group(1, &bucket.bind_group, &[]);
            self.draw_pointcloud(pointcloud);
        }
    }
}
