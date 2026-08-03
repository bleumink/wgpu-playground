use std::{collections::HashMap, marker::PhantomData, u32};

use bytemuck::{Pod, Zeroable};
use itertools::MultiUnzip;
use uuid::Uuid;

use crate::renderer::{context::RenderContext, material::{MaterialUniform, MaterialView}, mesh::{MeshView, PrimitiveView, SceneBuffer, TextureCoordinate}, pointcloud::{Pointcloud, PointcloudBuffer}, texture::{Sampler, TextureAtlas}};

pub type ResourceId = Uuid;

pub struct PrimitiveHandle {
    pub position_index: u32,
    pub surface_index: u32,
    pub color_index: u32,
    pub uv_indices: [u32; 4],
    pub first_index: u32,
    pub num_elements: u32,
    pub material_index: u32,
}

pub struct MeshHandle {
    pub primitives: Vec<PrimitiveHandle>,
}

pub struct PointcloudHandle {
    pub position_index: u32,
    pub color_index: u32,    
    pub num_elements: u32,
}

pub struct AttributeBuffer<T: Pod + Zeroable> {
    buffer: wgpu::Buffer,
    capacity: usize,
    used: usize,
    usage: wgpu::BufferUsages,
    label: Option<String>,
    is_dirty: bool,    
    _phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> AttributeBuffer<T> {
    pub fn new(capacity: usize, usage: wgpu::BufferUsages, label: Option<&str>, context: &RenderContext) -> Self {
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size: (capacity * std::mem::size_of::<T>()) as u64,
            usage: usage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            capacity,
            used: 0,
            usage,
            label: label.map(str::to_owned),
            is_dirty: false,
            _phantom: PhantomData,
        }
    }
    
    pub fn upload(&mut self, data: &[T], encoder: &mut wgpu::CommandEncoder, context: &RenderContext) -> u32 {
        if self.used + data.len() > self.capacity {
            self.resize(encoder, context);
        }

        let offset = self.used;
        
        context.queue.write_buffer(&self.buffer, (self.used * std::mem::size_of::<T>()) as u64, bytemuck::cast_slice(data));
        self.used += data.len();
        
        offset as u32
    }

    pub fn resize(&mut self, encoder: &mut wgpu::CommandEncoder, context: &RenderContext) {
        self.capacity = self.capacity * 2;
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: self.label.as_deref(),
            size: (self.capacity * std::mem::size_of::<T>()) as u64,
            usage: self.usage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, self.used as u64);
        self.buffer = buffer;
        self.is_dirty = true;
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn is_dirty(&mut self) -> bool {
        let dirty = self.is_dirty;
        self.is_dirty = false;
        dirty
    }

    pub fn is_empty(&self) -> bool {
        self.used == 0
    }
}

pub struct ResourcePool {
    positions: AttributeBuffer<[f32; 3]>,
    normals: AttributeBuffer<[f32; 3]>,
    tangents: AttributeBuffer<[f32; 4]>,    
    colors: AttributeBuffer<[f32; 4]>,
    uv_sets: AttributeBuffer<TextureCoordinate>,
    indices: AttributeBuffer<u32>,    

    materials: AttributeBuffer<MaterialUniform>,
    textures: TextureAtlas,

    mesh_handles: HashMap<ResourceId, MeshHandle>,
    pointcloud_handles: HashMap<ResourceId, PointcloudHandle>,
    material_indices: HashMap<ResourceId, u32>,

    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
}

impl ResourcePool {
    pub fn new(context: &RenderContext) -> Self {
        let vertex_capacity = 65536;
        let index_capacity = 65536 * 2;
        let uv_capacity = vertex_capacity;
        let material_capacity = 32;

        let positions = AttributeBuffer::new(
            vertex_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Geometry positions buffer"), 
            context
        );

        let normals = AttributeBuffer::new(
            vertex_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Geometry normal buffer"), 
            context
        );

        let tangents = AttributeBuffer::new(
            vertex_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Geometry tangents buffer"), 
            context
        );

        let colors = AttributeBuffer::new(
            vertex_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Geometry colors buffer"), 
            context
        );        

        let uv_sets = AttributeBuffer::new(
            uv_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Geometry uv buffer"), 
            context
        );

        let indices = AttributeBuffer::new(
            index_capacity, 
            wgpu::BufferUsages::INDEX, 
            Some("Geometry index buffer"), 
            context
        );

        let materials = AttributeBuffer::new(
            material_capacity, 
            wgpu::BufferUsages::STORAGE, 
            Some("Materials buffer"), 
            context
        );                

        let mut textures = TextureAtlas::new(RenderContext::ATLAS_SIZE, RenderContext::TEXTURE_PADDING_SIZE, context);
        let sampler_index = textures.samplers_mut().get_or_create(Sampler::default(), context);
        let sampler = textures.samplers().get_by_index(sampler_index as usize).unwrap();

        let layout = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Geometry bind group layout"),
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },              
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },              
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { 
                        sample_type: wgpu::TextureSampleType::Float { filterable: true }, 
                        view_dimension: wgpu::TextureViewDimension::D2, 
                        multisampled: false
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },                                  
            ]
        });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Resource bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: positions.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: normals.buffer().as_entire_binding(),
                },               
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: tangents.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: colors.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uv_sets.buffer().as_entire_binding(),
                },                                
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: materials.buffer().as_entire_binding(),
                },                    
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(textures.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(sampler),
                }   
            ]
        });

        Self {
            positions,
            normals,
            tangents,
            colors,
            uv_sets,
            indices,
            materials,
            textures,
            mesh_handles: HashMap::new(),
            pointcloud_handles: HashMap::new(),
            material_indices: HashMap::new(),
            bind_group,
            layout,
        }
    }

    pub fn get_mesh_handle(&self, id: &ResourceId) -> Option<&MeshHandle> {
        self.mesh_handles.get(id)
    }

    pub fn insert_material(&mut self, material: MaterialView, encoder: &mut wgpu::CommandEncoder, context: &RenderContext) -> ResourceId {
        let id = ResourceId::new_v4();
        let material = MaterialUniform::from_view(material, &mut self.textures, context);
        let index = self.materials.upload(&[material], encoder, context);
        self.material_indices.insert(id, index);

        id
    }

    pub fn insert_mesh(&mut self, mesh: MeshView, material_ids: &[ResourceId], encoder: &mut wgpu::CommandEncoder, context: &RenderContext) -> ResourceId {
        let primitives = mesh.primitives
            .into_iter()
            .map(|primitive| self.add_primitive(primitive, material_ids, encoder, context))
            .collect();

        let handle = MeshHandle { primitives };
        let id = ResourceId::new_v4();
        self.mesh_handles.insert(id, handle);
    
        id    
    }    

    // pub fn insert_pointcloud(&mut self, pointcloud: Pointcloud) -> ResourceId {
    //     let id = ResourceId::new_v4();
    //     pointcloud.
    // }

    pub fn sync(&mut self, context: &RenderContext) {
        let sampler_index = self.textures.samplers_mut().get_or_create(Sampler::default(), context);
        let sampler = self.textures.samplers().get_by_index(sampler_index as usize).unwrap();        
        self.bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Resource bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.positions.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.normals.buffer().as_entire_binding(),
                },               
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.tangents.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.colors.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.uv_sets.buffer().as_entire_binding(),
                },                                
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.materials.buffer().as_entire_binding(),
                },                    
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(self.textures.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(sampler),
                }                
            ]
        });        
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn indices(&self) -> &AttributeBuffer<u32> {
        &self.indices
    }

    pub fn is_dirty(&mut self) -> bool {
        if self.positions.is_dirty() 
            | self.normals.is_dirty()
            | self.tangents.is_dirty()
            | self.uv_sets.is_dirty()
            | self.indices.is_dirty()
            | self.textures.is_dirty()
            | self.materials.is_dirty()
        {
            true            
        } else {
            false
        }
    }

    fn add_primitive(&mut self, primitive: PrimitiveView, material_ids: &[ResourceId], encoder: &mut wgpu::CommandEncoder, context: &RenderContext) -> PrimitiveHandle {
        let (positions, normals, tangents): (Vec<_>, Vec<_>, Vec<_>) = primitive
            .vertices
            .iter()
            .map(|v| (v.position, v.normal, v.tangent))
            .multiunzip();

        let position_index = self.positions.upload(&positions, encoder, context);
        let normal_index = self.normals.upload(&normals, encoder, context);
        let _ = self.tangents.upload(&tangents, encoder, context);        
        let first_index = self.indices.upload(&primitive.indices, encoder, context);
                
        let mut uv_indices = [u32::MAX; RenderContext::MAX_UV_SETS];
        for (index, uv_set) in primitive.iter_uv_sets().enumerate() {
            if uv_set.is_empty() {
                continue;
            }
                         
            uv_indices[index] = self.uv_sets.upload(uv_set, encoder, context);
        }

        let material_index = *self.material_indices.get(
            &material_ids[primitive.material_index]
        ).unwrap_or(&0);

        PrimitiveHandle {
            position_index,
            surface_index: normal_index,
            color_index: u32::MAX,
            first_index,
            uv_indices,
            num_elements: primitive.indices.len() as u32,
            material_index,
        }        
    }    
}