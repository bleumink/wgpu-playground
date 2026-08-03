use std::{collections::{HashMap, HashSet}, hash::Hash, ops::Range};

use itertools::Itertools;
use uuid::Uuid;

use crate::{entity::EntityId, renderer::{
    component::{ComponentId, ComponentStore, RelationStore}, context::RenderContext, environment::EnvironmentMap, instance::{DrawCommand, Instance, InstancePool}, light::{Light, LightId, LightUniform}, material::Material, mesh::{Mesh, MeshVertex, MeshView, NodeView, Primitive, PrimitiveView, TextureCoordinate}, pipeline::PipelineCache, pointcloud::{DrawPointcloud, Pointcloud}, resource::{ResourceId, ResourcePool}, transform::TransformUniform
}};

pub type GeometryId = Uuid;
pub type RenderId = Uuid;
pub type NodeId = Uuid;

pub trait DrawScene<'a> {
    fn draw_scene(
        &mut self,
        scene: &'a SceneGraph,
        resources: &'a ResourcePool,
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
        resources: &'b ResourcePool,
        camera_bind_group: &'b wgpu::BindGroup,
        pipeline_cache: &'b PipelineCache,
    ) {
        self.set_bind_group(1, camera_bind_group, &[]);

        self.set_pipeline(scene.environment_map.pipeline());
        self.set_bind_group(0, scene.environment_map.bind_group(), &[]);
        self.draw(0..3, 0..1);
        
        self.set_bind_group(0, resources.bind_group(), &[]);
        self.set_bind_group(2, scene.bind_group(), &[]);
        self.set_bind_group(3, scene.environment_map.bind_group(), &[]);

        let pipeline = pipeline_cache.get("mesh").unwrap();
        self.set_pipeline(pipeline);
        
        self.set_vertex_buffer(0, scene.instance_pool.instances().slice(..));
        self.set_index_buffer(resources.indices().buffer().slice(..), wgpu::IndexFormat::Uint32);
        
        self.multi_draw_indexed_indirect(
            scene.instance_pool.commands(), 
            0, 
            scene.instance_pool.count()
        );
        

        // for batch in &scene.render_batches {
        //     let pipeline = pipeline_cache.get(batch.key.pipeline_id).unwrap();
        //     self.set_pipeline(pipeline);

        //     if let Some(renderable) = scene.renderables.get(&batch.key.render_id) {
        //         match renderable {
        //             Renderable::Mesh(handles) => {
        //                 self.set_vertex_buffer(7, scene.instance_pool.buffer().slice(..));
        //                 handles.iter().for_each(|handle| {
        //                     let geometry = scene.geometries.get_by_id(handle.geometry_index).unwrap();
        //                     let material = scene.materials.get_by_id(handle.material_index).unwrap();

        //                     if let Geometry::Primitive(primitive) = geometry {
        //                         self.draw_primitive_instanced(primitive, material, batch.instance_range());
        //                     }
        //                 });
        //             }
        //             Renderable::Pointcloud(handle) => {
        //                 self.set_vertex_buffer(1, scene.instance_pool.buffer().slice(..));
        //                 let geometry = scene.geometries.get_by_id(handle.geometry_index).unwrap();

        //                 if let Geometry::Pointcloud(pointcloud) = geometry {
        //                     self.draw_pointcloud(pointcloud, batch.instance_range());
        //                 }
        //             }
        //         }
        //     }
        // }
    }
}

// pub enum Renderable {
//     Mesh(Vec<PrimitiveHandle>),
//     Pointcloud(PointcloudHandle),
// }

// impl Renderable {
//     pub fn as_str(&self) -> &'static str {
//         match self {
//             Self::Mesh(_) => "mesh",
//             Self::Pointcloud(_) => "pointcloud",
//         }
//     }
// }

// pub enum Geometry {
//     Primitive(Primitive),
//     Pointcloud(Pointcloud),
// }


// pub struct PrimitiveHandle {
//     pub geometry_index: ComponentId<Geometry>,
//     pub material_index: ComponentId<Material>,
// }

// pub struct PointcloudHandle {
//     pub geometry_index: ComponentId<Geometry>,
// }

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NormalUniform([[f32; 4]; 4]);

impl NormalUniform {
    pub fn new(transform: glam::Mat4) -> Self {
        let normal_matrix = transform.inverse().transpose();
        Self(normal_matrix.to_cols_array_2d())
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct BatchKey {
    pub pipeline_id: &'static str,
    pub render_id: RenderId,
}

#[derive(Debug)]
pub struct RenderBatch {
    pub key: BatchKey,
    pub instance_offset: u32,
    pub instance_count: u32,
}

// impl RenderBatch {
//     pub fn instance_range(&self) -> Range<u32> {
//         self.instance_offset..self.instance_offset + self.instance_count
//     }
// }

pub struct SceneGraph {
    pub nodes: HashSet<NodeId>,
    pub root_nodes: HashSet<NodeId>,    
    pub node_to_parent: HashMap<NodeId, NodeId>,
    pub node_to_children: HashMap<NodeId, Vec<NodeId>>,
    pub node_to_mesh: HashMap<NodeId, ResourceId>,
        
    pub transforms: ComponentStore<TransformUniform>,    
    pub normals: ComponentStore<NormalUniform>,    
    pub lights: ComponentStore<LightUniform>,
            
    pub active_nodes: HashMap<EntityId, NodeId>,
    pub entity_to_transform: HashMap<EntityId, ComponentId<TransformUniform>>,
    pub entity_to_normal: HashMap<EntityId, ComponentId<NormalUniform>>,
    pub entity_to_light: HashMap<EntityId, ComponentId<LightUniform>>,
    
    // pub lights_transform_index: RelationStore<LightUniform, TransformUniform>,

    pub environment_map: EnvironmentMap,
    pub instance_pool: InstancePool,
    pub render_batches: Vec<RenderBatch>,
    pub debug_id: RenderId,
    pub bind_group: wgpu::BindGroup,
    pub layout: wgpu::BindGroupLayout,
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
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
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

        let instance_pool = InstancePool::new(2048, &context);

        let transforms = ComponentStore::new(64, wgpu::ShaderStages::VERTEX, context);
        let normals = ComponentStore::new(64, wgpu::ShaderStages::VERTEX, context);
        let lights = ComponentStore::new(64, wgpu::ShaderStages::FRAGMENT, context);

        // let node_parent = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        // let node_mesh = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        
        // let node_transform = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        // let node_normal= RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        // let node_light= RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        // let lights_transform_index = RelationStore::new(64, wgpu::ShaderStages::FRAGMENT, context);

        // let mesh = Mesh::unit_cube(context);
        // let handles = mesh
        //     .primitives
        //     .into_iter()
        //     .map(|primitive| PrimitiveHandle {
        //         geometry_index: geometries.add(GeometryId::new_v4(), Geometry::Primitive(primitive)),
        //         material_index: ComponentId::new(0),
        //     })
        //     .collect::<Vec<_>>();
        let debug_id = RenderId::new_v4();
        // renderables.add(debug_id, Renderable::Mesh(handles));

        let bind_group = Self::create_bind_group(
            &[
                transforms.buffer(),
                normals.buffer(),
                lights.buffer(),
                // lights_transform_index.buffer(),
            ],
            &layout,
            context,
        );

        Self {
            nodes: HashSet::new(),
            root_nodes: HashSet::new(),
            active_nodes: HashMap::new(),
            node_to_children: HashMap::new(),
            node_to_parent: HashMap::new(),
            node_to_mesh: HashMap::new(),
            entity_to_transform: HashMap::new(),
            entity_to_normal: HashMap::new(),
            entity_to_light: HashMap::new(),
            transforms,
            lights,
            normals,

            environment_map: EnvironmentMap::default(context),
            instance_pool,
            render_batches: Vec::new(),
            debug_id,
            bind_group,
            layout,
        }
    }

    // pub fn add_material(&mut self, material: Material) -> ComponentId<Material> {
    //     self.materials.add(MaterialId::new_v4(), material)
    // }

    // pub fn add_mesh(&mut self, mesh: MeshView, material_ids: &[u32], context: &RenderContext) -> MeshId {                
    //     self.meshes.insert(mesh, material_ids, context)
        
        
        // let handles = mesh
        //     .primitives
        //     .into_iter()
        //     .map(|primitive| PrimitiveHandle {
        //         material_index: material_components[primitive.material_index],
        //         geometry_index: self.add_geometry(Geometry::Primitive(primitive)),
        //     })
        //     .collect::<Vec<_>>();

        // let renderable = Renderable::Mesh(handles);
        // self.add_renderable(renderable)
    // }

    // pub fn add_pointcloud(&mut self, pointcloud: Pointcloud) -> RenderId {
    //     let renderable = Renderable::Pointcloud(PointcloudHandle {
    //         geometry_index: self.add_geometry(Geometry::Pointcloud(pointcloud)),
    //     });
    //     self.add_renderable(renderable)
    // }

    // pub fn add_geometry(&mut self, geometry: Geometry) -> ComponentId<Geometry> {
    //     self.geometries.add(GeometryId::new_v4(), geometry)
    // }

    // pub fn add_renderable(&mut self, renderable: Renderable) -> RenderId {
    //     let id = RenderId::new_v4();
    //     self.renderables.add(id, renderable);
    //     id
    // }    

    pub fn import_hierarchy(&mut self, root_nodes: impl Iterator<Item = NodeView>, mesh_ids: &[ResourceId], context: &RenderContext) -> Vec<NodeId> {
        root_nodes.map(|node| {
            self.add_node(node, mesh_ids, None, context)
        })
        .collect()
    }

    pub fn get_transform(&self, id: &NodeId) -> Option<glam::Mat4> {        
        self.transforms.get(id).map(|uniform| uniform.to_mat4())
    }

    pub fn spawn_node(&mut self, entity_id: EntityId, node_id: NodeId, transform: glam::Mat4, context: &RenderContext) {
        self.active_nodes.insert(entity_id, node_id);
        
        let transform_uniform = TransformUniform::new(transform);
        let transform_id = self.transforms.add(entity_id, transform_uniform, context);
        self.node_to_transform.insert(entity_id, transform_id);

        let normal_uniform = NormalUniform::new(transform);
        let normal_id = self.normals.add(entity_id, normal_uniform, context);
        self.node_to_normal.insert(entity_id, normal_id);
    }

    pub fn add_node(&mut self, node: NodeView, mesh_ids: &[ResourceId], parent: Option<NodeId>, context: &RenderContext) -> NodeId {
        let id = NodeId::new_v4();
        self.nodes.insert(id);
        
        let transform_uniform = TransformUniform::new(node.transform);        
        let transform_index = self.transforms.add(id, transform_uniform, context);
        self.node_to_transform.insert(id, transform_index);
        
        if let Some(index) = node.mesh_index {                                    
            let mesh_id = mesh_ids[index];
            self.node_to_mesh.insert(id, mesh_id);
        }

        if let Some(parent_id) = parent {
            self.node_to_parent.insert(id, parent_id);
            self.node_to_children
                .entry(parent_id)
                .or_default()
                .push(id);
        } else {
            self.root_nodes.insert(id);
        }

        for child in node.children {
            self.add_node(child, mesh_ids, Some(id), context);
        }
    
        // self.build_render_batches(context);
        id
    }

    pub fn add_light(&mut self, entity_id: Uuid, light: Light, context: &RenderContext) {
        let transform = TransformUniform::new(light.to_transform());
        let transform_id = self.transforms.add(entity_id, transform, context);

        let uniform = light.to_light_uniform(transform_id.index());        
        let light_id = self.lights.add(entity_id, uniform, context);

        self.entity_to_light.insert(entity_id, light_id);
    }

    pub fn set_environment_map(&mut self, environment_map: EnvironmentMap) {
        self.environment_map = environment_map;
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn build_draw_commands(&mut self, meshes: &ResourcePool, context: &RenderContext) {
        let mut instances = Vec::new();
        let mut commands = Vec::new();

        let mesh_to_nodes: HashMap<ResourceId, Vec<NodeId>> = self.nodes
            .iter()
            .fold(HashMap::new(), |mut accumulator, node_id| {
                let mesh_id = match self.node_to_mesh.get(node_id) {
                    Some(id) => id,
                    None => return accumulator, 
                };
                            
                accumulator.entry(*mesh_id).or_default().push(*node_id);
                accumulator
        });

        for (mesh_id, node_ids) in &mesh_to_nodes {
            let mesh_handle = match meshes.get_mesh_handle(mesh_id) {
                Some(handle) => handle,
                None => continue
            };
            
            for primitive in &mesh_handle.primitives {
                let first_instance = instances.len() as u32;

                for node_id in node_ids {
                    let transform_index = match self.node_to_transform.get(node_id) {
                        Some(id) => id.index(),
                        None => continue
                    };

                    let normal_index = match self.node_to_normal.get(node_id) {
                        Some(id) => id.index(),
                        None => continue
                    };

                    let instance = Instance {
                        position_offset: primitive.position_index,
                        surface_offset: primitive.surface_index,
                        uv_offsets: primitive.uv_indices,
                        material_index: primitive.material_index,
                        transform_index,
                        normal_index,
                    };

                    instances.push(instance);
                }

                let instance_count = (instances.len() as u32) - first_instance;
                if instance_count > 0 {
                    let command = DrawCommand {
                        index_count: primitive.num_elements,
                        instance_count,
                        first_index: primitive.first_index,
                        base_vertex: 0,
                        first_instance,
                    };

                    commands.push(command);
                }
            }
        }

        self.instance_pool.upload_commands(&instances, &commands, context);
    }

    // pub fn build_render_batches(&mut self, context: &RenderContext) {
    //     let mut batches: HashMap<BatchKey, Vec<Instance>> = HashMap::new();

    //     // Nodes
    //     for (entity, render_index, render_id) in self.nodes.iter_with_index() {
    //         if let Some(transform_index) = self.node_transform_index.get_mapping(render_index)
    //             && let Some(normal_index) = self.node_normal_index.get_mapping(render_index)
    //         {
    //             if let Some(renderable) = self.renderables.get(render_id) {
    //                 let pipeline_id = renderable.as_str();
    //                 let key = BatchKey {
    //                     render_id: *render_id,
    //                     pipeline_id,
    //                 };

    //                 batches.entry(key).or_default().push(Instance {
    //                     transform_index,
    //                     normal_index,
    //                 });
    //             }
    //         }
    //     }

    //     // Lights - Debug
    //     for (light_id, light_index, uniform) in self.lights.iter_with_index() {
    //         if uniform.kind != 1 {
    //             continue;
    //         }

    //         if let Some(transform_index) = self.lights_transform_index.get_mapping(light_index) {
    //             if let Some(renderable) = self.renderables.get(&self.debug_id) {
    //                 let key = BatchKey {
    //                     render_id: self.debug_id,
    //                     pipeline_id: "light",
    //                 };

    //                 batches.entry(key).or_default().push(Instance {
    //                     transform_index,
    //                     normal_index: 0,
    //                 });
    //             }
    //         }
    //     }

    //     let mut render_batches = Vec::new();
    //     for (key, instances) in batches {
    //         let instance_offset = self.instance_pool.upload(&instances, context);
    //         let instance_count = instances.len();

    //         render_batches.push(RenderBatch {
    //             key,
    //             instance_offset: instance_offset as u32,
    //             instance_count: instance_count as u32,
    //         })
    //     }

    //     render_batches.sort_by_key(|batch| (batch.key.pipeline_id, batch.key.render_id));
    //     self.render_batches = render_batches;
    // }

    pub fn sync(&mut self, context: &RenderContext) {
        if self.transforms.is_dirty()
            || self.lights.is_dirty()                        
            || self.normals.is_dirty()            
        {
            let bind_group = Self::create_bind_group(
                &[
                    self.transforms.buffer(),
                    self.normals.buffer(),
                    self.lights.buffer(),
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