use std::{collections::HashMap, ops::Range};

use uuid::Uuid;

use crate::{
    component::{ComponentStore, LocalComponentStore, RelationStore}, context::RenderContext, entity::EntityId, instance::{InstancePool, RawInstance}, light::{Light, LightBuffer, LightId, LightUniform}, material::MaterialInstance, mesh::{DrawMesh, Mesh, Primitive, Scene}, pointcloud::{DrawPointcloud, Pointcloud}, renderer::{PipelineCache, RenderId}, transform::{TransformBuffer, TransformUniform}
};

#[derive(Clone, Debug)]
pub enum RenderKind {
    Mesh(Mesh, MaterialsId),
    Pointcloud(Pointcloud),
    DebugLight,
}

impl RenderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mesh(_, _) => "mesh",
            Self::Pointcloud(_) => "pointcloud",
            Self::DebugLight => "light",
        }
    }
}


pub struct MaterialData {
    scene_id: SceneId,
    materials: Vec<MaterialInstance>,
}

#[derive(Clone, Debug)]
pub struct MaterialsId(SceneId, usize);

pub type GeometryId = Uuid;
pub type SceneId = Uuid;

pub enum Geometry {
    Mesh(Mesh),
    Pointcloud(Pointcloud),
}

pub struct GeometryStore(HashMap<GeometryId, Geometry>);
impl GeometryStore {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, geometry: Geometry) -> GeometryId {
        let id = GeometryId::new_v4();
        self.0.insert(id, geometry);
        id
    }

    pub fn get(&self, id: &GeometryId) -> Option<&Geometry> {
        self.0.get(id)
    }
}

#[derive(Clone, Debug)]
pub struct Renderable {
    geometry: GeometryId,
    material: Option<MaterialsId>,
}

pub struct EntityTransformData(pub EntityId, pub usize);

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct BatchKey {
    pub pipeline_id: &'static str,
    pub render_id: RenderId,        
}

pub struct RenderBatch {
    pub key: BatchKey,
    pub instances: Vec<RawInstance>,    
    pub instance_offset: usize,
    pub instance_count: usize,
}

impl RenderBatch {
    pub fn instance_range(&self) -> Range<u32> {        
        self.instance_offset as u32..(self.instance_offset + (self.instance_count * std::mem::size_of::<RawInstance>())) as u32
    }
}

pub struct RenderGroup {
    kind: RenderKind,
    instances: Vec<EntityTransformData>,
    pipeline: wgpu::RenderPipeline,
}

impl RenderGroup {
    pub fn new(kind: RenderKind, pipeline: wgpu::RenderPipeline) -> Self {
        Self {
            kind,
            instances: Vec::new(),
            pipeline,
        }
    }
}

pub struct SceneGraph {
    pub transforms: ComponentStore<TransformUniform>,
    pub renderables: LocalComponentStore<Renderable>,    
    pub lights: ComponentStore<LightUniform>,
    
    pub renderable_transform_index: RelationStore<RenderId, TransformUniform>,
    pub lights_transform_index: RelationStore<LightUniform, TransformUniform>,
    
    pub geometries: GeometryStore,

    pub instance_pool: InstancePool,
    pub render_batches: Vec<RenderBatch>,    
    pub render_groups: HashMap<RenderId, RenderGroup>,
    pub materials: Vec<MaterialData>,
    pub bind_group: wgpu::BindGroup,
    pub layout: wgpu::BindGroupLayout,
    pub debug_mesh: Mesh,
}

impl SceneGraph {
    pub fn new(context: &RenderContext) -> Self {
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Scene bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },               
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let transforms = ComponentStore::new(64, context);
        let renderables = LocalComponentStore::new();
        let renderable_transform_index = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        let lights = ComponentStore::new(64, context);
        let lights_transform_index = RelationStore::new(64, wgpu::ShaderStages::VERTEX_FRAGMENT, context);
        let instance_pool = InstancePool::new(1024, &context);

        let bind_group = Self::create_bind_group(
            &[
                transforms.buffer(),
                instance_pool.buffer(),
                lights.buffer(),
                lights_transform_index.buffer(),
            ],
            &layout,
            context,
        );

        Self {
            layout,
            transforms,
            renderables,
            renderable_transform_index,
            lights,
            lights_transform_index,
            
            instance_pool,
            render_batches: Vec::new(),
            geometries: GeometryStore::new(),
            bind_group,
            render_groups: HashMap::new(),
            materials: Vec::new(),
            debug_mesh: Mesh::unit_cube(&context),
        }
    }

    pub fn add_materials(&mut self, materials: Vec<MaterialInstance>) -> MaterialsId {
        let scene_id = Uuid::new_v4();
        let index = self.materials.len();
        self.materials.push(MaterialData {
            scene_id: scene_id.clone(),
            materials,
        });

        MaterialsId(scene_id, index)
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn add_light(&mut self, entity: EntityId, light: Light, context: &RenderContext) {
        let (uniform, transform) = light.to_parts();
        let transform_index = self.transforms.add(entity, transform, context);
        let light_index = self.lights.add(entity, uniform, context);
        self.lights_transform_index.link(light_index, transform_index, context);
    }

    pub fn add_group(&mut self, kind: RenderKind, pipeline: wgpu::RenderPipeline) -> RenderId {
        let id = RenderId::new_v4();
        let bucket = RenderGroup::new(kind, pipeline);
        self.render_groups.insert(id, bucket);
        id
    }

    pub fn add_renderable(&mut self, renderable: Renderable) {
        self.geometries.insert(renderable.geometry)
    }

    pub fn add_instance(
        &mut self,
        entity: EntityId,
        render_id: RenderId,
        transform: glam::Mat4,
        context: &RenderContext,
    ) {
        let transform_uniform = TransformUniform::new(transform);
        let transform_index = self.transforms.add(entity, transform_uniform, context);

        let render_index = self.renderables.add(entity, render_id);
        self.renderable_transform_index
            .link(render_index, transform_index, context);

        self.build_render_batches(context);
    }

    // pub fn iter_meshes(&self) -> impl Iterator<Item = (&Mesh, &Vec<MaterialInstance>, &RenderGroup, u32)> {
    //     self.render_groups.iter().filter_map(|(id, bucket)| {
    //         if let RenderKind::Mesh(mesh, material_id) = &bucket.kind {
    //             let instance_count = self
    //                 .renderables
    //                 .components()
    //                 .iter()
    //                 .filter(|&render_id| render_id == id)
    //                 .count();
    //             let materials = &self.materials[material_id.1].materials;
    //             Some((mesh, materials, bucket, instance_count as u32))
    //         } else {
    //             None
    //         }
    //     })
    // }

    // pub fn iter_pointclouds(&self) -> impl Iterator<Item = (&Pointcloud, &RenderGroup, u32)> {
    //     self.render_groups.iter().filter_map(|(id, bucket)| {
    //         if let RenderKind::Pointcloud(pointcloud) = &bucket.kind {
    //             let instance_count = self
    //                 .renderables
    //                 .components()
    //                 .iter()
    //                 .filter(|&render_id| render_id == id)
    //                 .count();
    //             Some((pointcloud, bucket, instance_count as u32))
    //         } else {
    //             None
    //         }
    //     })
    // }

    pub fn build_render_batches(&mut self, context: &RenderContext) {
        let mut batches: HashMap<BatchKey, RenderBatch> = HashMap::new();
        let mut pool = Vec::new();


        for (entity, render_index, render_id) in self.renderables.iter_with_index() {
            if let Some(transform_index) = self.renderable_transform_index.get_mapping(render_index) {
                let offset = pool.len();
                
                let instance = RawInstance {transform_index, material_index: 0 };
                pool.push(instance);
                
                let pipeline_id = self.render_groups.get(render_id).unwrap().kind.as_str();
                let key = BatchKey { render_id: *render_id, pipeline_id };
                
                let batch = batches.entry(key.clone()).or_insert(RenderBatch { 
                    key, 
                    instances: Vec::new(), 
                    instance_offset: offset, 
                    instance_count: 0,
                });

                batch.instances.push(RawInstance { 
                    transform_index, 
                    material_index: 0 
                });

                batch.instance_count += 1;
            } else {
                log::warn!("Renderable {:?} has no transform mapping", entity);
            }
        }

        let pool_offset = self.instance_pool.upload(&pool, context);

        let mut sorted_batches = batches.into_values().collect::<Vec<_>>();
        sorted_batches.sort_by_key(|batch| (batch.key.pipeline_id, batch.key.render_id));

        let mut offset = 0;
        for batch in &mut sorted_batches {
            batch.instance_offset = offset;
            offset += pool_offset + (batch.instance_count * std::mem::size_of::<RawInstance>());
        }        
        
        self.render_batches = sorted_batches;
    }

    pub fn sync(&mut self, context: &RenderContext) {
        if self.transforms.is_dirty()
            || self.lights.is_dirty()
            || self.renderable_transform_index.is_dirty()
            || self.lights_transform_index.is_dirty()
        {
            let bind_group = Self::create_bind_group(
                &[
                    self.transforms.buffer(),
                    self.instance_pool.buffer(),
                    self.lights.buffer(),
                    self.lights_transform_index.buffer(),
                ],
                &self.layout,
                context,
            );

            self.bind_group = bind_group;
        }
    }

    fn create_bind_group(
        buffers: &[&wgpu::Buffer],
        layout: &wgpu::BindGroupLayout,
        context: &RenderContext,
    ) -> wgpu::BindGroup {
        let entries = buffers
            .iter()
            .enumerate()
            .map(|(index, &buffer)| wgpu::BindGroupEntry {
                binding: index as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>();

        context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene bind group"),
            layout,
            entries: &entries,
        })
    }
}

pub trait DrawScene<'a> {
    fn draw_scene(
        &mut self,
        scene: &'a SceneGraph,
        camera_bind_group: &'a wgpu::BindGroup,
        pipeline_cache: &'a PipelineCache,
    );
}

impl<'a, 'b> DrawScene<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    fn draw_scene(
        &mut self,
        scene: &'b SceneGraph,
        camera_bind_group: &'b wgpu::BindGroup,
        pipeline_cache: &'b PipelineCache,
    ) {
        self.set_bind_group(1, camera_bind_group, &[]);
        self.set_bind_group(2, scene.bind_group(), &[]);

        let light_pipeline = pipeline_cache.get("light").unwrap();
        self.set_pipeline(light_pipeline);
        self.draw_mesh_instanced(&scene.debug_mesh, &scene.materials[0].materials, 0..1 as u32);

        for batch in &scene.render_batches {            
            let pipeline = pipeline_cache.get(batch.key.pipeline_id).unwrap();
            self.set_pipeline(pipeline);
                        
            if let Some(geometry) = scene.render_groups.get(&batch.key.render_id) {
                match &geometry.kind {
                    RenderKind::Mesh(mesh, material_id) => {
                        let materials = &scene.materials[material_id.1].materials;
                        self.draw_mesh_instanced(&mesh, materials, batch.instance_range());
                    },
                    RenderKind::Pointcloud(pointcloud) => {
                        self.draw_pointcloud(&pointcloud, batch.instance_range());
                    },
                    _ => (),
                }
            }
        }

        // for (mesh, materials, bucket, instance_count) in scene.iter_meshes() {
        //     self.set_pipeline(&bucket.pipeline);
        //     self.draw_mesh_instanced(mesh, materials, 0..instance_count);
        // }

        // self.set_bind_group(0, camera_bind_group, &[]);
        // self.set_bind_group(1, scene.bind_group(), &[]);
        // for (pointcloud, bucket, instance_count) in scene.iter_pointclouds() {
        //     self.set_pipeline(&bucket.pipeline);
        //     self.draw_pointcloud(pointcloud);
        // }
    }
}
