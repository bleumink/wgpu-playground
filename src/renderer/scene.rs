use std::{collections::HashMap, hash::Hash, ops::Range};

use uuid::Uuid;

use crate::renderer::{
    component::{ComponentId, ComponentStore, HostComponentStore, RelationStore},
    context::RenderContext,
    environment::EnvironmentMap,
    instance::{Instance, InstancePool},
    light::{Light, LightId, LightUniform},
    material::Material,
    mesh::{DrawMesh, Mesh, Primitive, Scene},
    pipeline::PipelineCache,
    pointcloud::{DrawPointcloud, Pointcloud},
    transform::TransformUniform,
};

pub type MaterialId = Uuid;
pub type GeometryId = Uuid;
pub type RenderId = Uuid;

pub enum Renderable {
    Mesh(Vec<PrimitiveHandle>),
    Pointcloud(PointcloudHandle),
}

impl Renderable {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mesh(_) => "mesh",
            Self::Pointcloud(_) => "pointcloud",
        }
    }
}

pub enum Geometry {
    Primitive(Primitive),
    Pointcloud(Pointcloud),
}

pub struct PrimitiveHandle {
    pub geometry_index: ComponentId<Geometry>,
    pub material_index: ComponentId<Material>,
}

pub struct PointcloudHandle {
    pub geometry_index: ComponentId<Geometry>,
}

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

impl RenderBatch {
    pub fn instance_range(&self) -> Range<u32> {
        self.instance_offset..self.instance_offset + self.instance_count
    }
}

pub struct SceneGraph {
    pub nodes: HostComponentStore<RenderId>,
    pub renderables: HostComponentStore<Renderable>,
    pub geometries: HostComponentStore<Geometry>,
    pub materials: HostComponentStore<Material>,

    pub normals: ComponentStore<NormalUniform>,
    pub transforms: ComponentStore<TransformUniform>,
    pub lights: ComponentStore<LightUniform>,

    pub node_transform_index: RelationStore<RenderId, TransformUniform>,
    pub node_normal_index: RelationStore<RenderId, NormalUniform>,
    pub lights_transform_index: RelationStore<LightUniform, TransformUniform>,

    pub environment_map: Option<EnvironmentMap>,
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
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
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

        let node_transform_index = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        let node_normal_index = RelationStore::new(64, wgpu::ShaderStages::VERTEX, context);
        let lights_transform_index = RelationStore::new(64, wgpu::ShaderStages::FRAGMENT, context);

        let mut renderables = HostComponentStore::new();
        let mut geometries = HostComponentStore::new();
        let materials = HostComponentStore::new();

        let mesh = Mesh::unit_cube(context);
        let handles = mesh
            .primitives
            .into_iter()
            .map(|primitive| PrimitiveHandle {
                geometry_index: geometries.add(GeometryId::new_v4(), Geometry::Primitive(primitive)),
                material_index: ComponentId::new(0),
            })
            .collect::<Vec<_>>();
        let debug_id = RenderId::new_v4();
        renderables.add(debug_id, Renderable::Mesh(handles));

        let bind_group = Self::create_bind_group(
            &[
                transforms.buffer(),
                normals.buffer(),
                lights.buffer(),
                lights_transform_index.buffer(),
            ],
            &layout,
            context,
        );

        Self {
            nodes: HostComponentStore::new(),
            transforms,
            renderables,
            node_transform_index,
            lights,
            lights_transform_index,
            normals,
            node_normal_index,

            geometries,
            materials,

            environment_map: None,
            instance_pool,
            render_batches: Vec::new(),
            debug_id,
            bind_group,
            layout,
        }
    }

    pub fn add_material(&mut self, material: Material) -> ComponentId<Material> {
        self.materials.add(MaterialId::new_v4(), material)
    }

    pub fn add_mesh(&mut self, mesh: Mesh, material_components: &[ComponentId<Material>]) -> RenderId {
        let handles = mesh
            .primitives
            .into_iter()
            .map(|primitive| PrimitiveHandle {
                material_index: material_components[primitive.material_index],
                geometry_index: self.add_geometry(Geometry::Primitive(primitive)),
            })
            .collect::<Vec<_>>();

        let renderable = Renderable::Mesh(handles);
        self.add_renderable(renderable)
    }

    pub fn add_pointcloud(&mut self, pointcloud: Pointcloud) -> RenderId {
        let renderable = Renderable::Pointcloud(PointcloudHandle {
            geometry_index: self.add_geometry(Geometry::Pointcloud(pointcloud)),
        });
        self.add_renderable(renderable)
    }

    pub fn add_geometry(&mut self, geometry: Geometry) -> ComponentId<Geometry> {
        self.geometries.add(GeometryId::new_v4(), geometry)
    }

    pub fn add_renderable(&mut self, renderable: Renderable) -> RenderId {
        let id = RenderId::new_v4();
        self.renderables.add(id, renderable);
        id
    }

    pub fn add_node(&mut self, entity: Uuid, handle: RenderId, transform: glam::Mat4, context: &RenderContext) {
        let transform_uniform = TransformUniform::new(transform);
        let transform_index = self.transforms.add(entity, transform_uniform, context);

        let node_index = self.nodes.add(entity, handle);
        self.node_transform_index.link(node_index, transform_index, context);

        let normal_uniform = NormalUniform::new(transform);
        let normal_index = self.normals.add(entity, normal_uniform, context);
        self.node_normal_index.link(node_index, normal_index, context);

        self.build_render_batches(context);
    }

    pub fn add_light(&mut self, entity: Uuid, light: Light, context: &RenderContext) {
        let (uniform, transform) = light.to_parts();
        let transform_index = self.transforms.add(entity, transform, context);
        let light_index = self.lights.add(entity, uniform, context);
        self.lights_transform_index.link(light_index, transform_index, context);
    }

    pub fn set_environment_map(&mut self, environment_map: EnvironmentMap) {
        self.environment_map = Some(environment_map);
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn build_render_batches(&mut self, context: &RenderContext) {
        let mut batches: HashMap<BatchKey, Vec<Instance>> = HashMap::new();

        // Nodes
        for (entity, render_index, render_id) in self.nodes.iter_with_index() {
            if let Some(transform_index) = self.node_transform_index.get_mapping(render_index)
                && let Some(normal_index) = self.node_normal_index.get_mapping(render_index)
            {
                if let Some(renderable) = self.renderables.get(render_id) {
                    let pipeline_id = renderable.as_str();
                    let key = BatchKey {
                        render_id: *render_id,
                        pipeline_id,
                    };

                    batches.entry(key).or_default().push(Instance {
                        transform_index,
                        normal_index,
                    });
                }
            }
        }

        // Lights - Debug
        for (light_id, light_index, uniform) in self.lights.iter_with_index() {
            if uniform.kind != 1 {
                continue;
            }

            if let Some(transform_index) = self.lights_transform_index.get_mapping(light_index) {
                if let Some(renderable) = self.renderables.get(&self.debug_id) {
                    let key = BatchKey {
                        render_id: self.debug_id,
                        pipeline_id: "light",
                    };

                    batches.entry(key).or_default().push(Instance {
                        transform_index,
                        normal_index: 0,
                    });
                }
            }
        }

        let mut render_batches = Vec::new();
        for (key, instances) in batches {
            let instance_offset = self.instance_pool.upload(&instances, context);
            let instance_count = instances.len();

            render_batches.push(RenderBatch {
                key,
                instance_offset: instance_offset as u32,
                instance_count: instance_count as u32,
            })
        }

        render_batches.sort_by_key(|batch| (batch.key.pipeline_id, batch.key.render_id));
        self.render_batches = render_batches;
    }

    pub fn sync(&mut self, context: &RenderContext) {
        if self.transforms.is_dirty()
            || self.lights.is_dirty()
            || self.node_transform_index.is_dirty()
            || self.lights_transform_index.is_dirty()
            || self.normals.is_dirty()
            || self.node_normal_index.is_dirty()
        {
            let bind_group = Self::create_bind_group(
                &[
                    self.transforms.buffer(),
                    self.normals.buffer(),
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

        if let Some(environment_map) = &scene.environment_map {
            self.set_pipeline(environment_map.pipeline());
            self.set_bind_group(0, environment_map.bind_group(), &[]);
            self.draw(0..3, 0..1);
        }

        self.set_bind_group(2, scene.bind_group(), &[]);
        self.set_vertex_buffer(7, scene.instance_pool.buffer().slice(..));

        for batch in &scene.render_batches {
            let pipeline = pipeline_cache.get(batch.key.pipeline_id).unwrap();
            self.set_pipeline(pipeline);

            if let Some(renderable) = scene.renderables.get(&batch.key.render_id) {
                match renderable {
                    Renderable::Mesh(handles) => {
                        self.set_vertex_buffer(7, scene.instance_pool.buffer().slice(..));
                        handles.iter().for_each(|handle| {
                            let geometry = scene.geometries.get_by_id(handle.geometry_index).unwrap();
                            let material = scene.materials.get_by_id(handle.material_index).unwrap();

                            if let Geometry::Primitive(primitive) = geometry {
                                self.draw_primitive_instanced(primitive, material, batch.instance_range());
                            }
                        });
                    }
                    Renderable::Pointcloud(handle) => {
                        self.set_vertex_buffer(1, scene.instance_pool.buffer().slice(..));
                        let geometry = scene.geometries.get_by_id(handle.geometry_index).unwrap();

                        if let Geometry::Pointcloud(pointcloud) = geometry {
                            self.draw_pointcloud(pointcloud, batch.instance_range());
                        }
                    }
                }
            }
        }
    }
}
