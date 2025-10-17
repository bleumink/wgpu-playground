use std::{
    io::{BufReader, Cursor},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};
use image::EncodableLayout;
use wgpu::util::DeviceExt;

use crate::{
    asset::ResourcePath, binary::BlobBuilder, context::RenderContext, material::{Material, MaterialHandle, MaterialView, TextureSlot}, texture::{Sampler, Texture, TextureFormat, TextureView}, vertex::Vertex
};

const MAT_SWAP_YZ: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub trait DrawMesh<'a> {
    // fn draw_mesh(&mut self, mesh: &'a Mesh, material: &'a Material);
    fn draw_primitive_instanced(&mut self, primitive: &'a PrimitiveHandle, material: &'a MaterialHandle, instances: Range<u32>);
    // fn draw_model(&mut self, model: &'a Model);
    fn draw_mesh_instanced(&mut self, mesh: &'a Mesh, instances: Range<u32>);
}

impl<'a, 'b> DrawMesh<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    // fn draw_mesh(&mut self, mesh: &'b Mesh, material: &'b Material) {
    //     self.draw_mesh_instanced(mesh, material, 0..1);
    // }

    fn draw_primitive_instanced(&mut self, primitive: &'b PrimitiveHandle, material: &'b MaterialHandle, instances: Range<u32>) {
        self.set_vertex_buffer(0, primitive.vertex_buffer.slice(..));
        self.set_vertex_buffer(1, primitive.uv_buffers[0].slice(..));

        self.set_index_buffer(primitive.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        self.set_bind_group(0, &material.bind_group, &[]);

        self.draw_indexed(0..primitive.num_elements, 0, instances);
    }

    // fn draw_model(&mut self, model: &'b Model) {
    //     let instance_range = if let Some(instances) = &model.instances {
    //         self.set_vertex_buffer(1, instances.buffer.slice(..));
    //         0..instances.data.len() as u32
    //     } else {
    //         0..1
    //     };

    //     self.draw_model_instanced(model, instance_range);
    // }

    fn draw_mesh_instanced(&mut self, mesh: &'b Mesh, instances: Range<u32>) {
        for primitive in &mesh.primitives {
            let material = &mesh.materials[primitive.material];
            self.draw_primitive_instanced(primitive, material, instances.clone());
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    // pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex for MeshVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureCoordinate([f32; 2]);
impl Vertex for TextureCoordinate {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                }
            ]
        }
    }
}

// pub struct ModelView<'a> {
//     pub meshes: &'a [MeshView<'a>],
//     pub materials: &'a [MaterialView<'a>],
// }

pub struct PrimitiveView<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],    
    pub material: usize,
    uv_headers: &'a [TexCoordHeader],
    raw_uv_sets: &'a [u8],
}

impl<'a> PrimitiveView<'a> {
    // pub fn get_uv_set(&self, index: usize) -> Option<&'a [TextureCoordinate]> {        
    //     if let Some(header) = self.uv_headers.get(index) {
    //         let uv_set_end = header.offset + header.count * std::mem::size_of::<TextureCoordinate>();
    //         let slice = &self.raw_uv_sets[header.offset..uv_set_end];

    //         Some(bytemuck::cast_slice(slice))
    //     } else {
    //         None
    //     }
    // }

    pub fn iter_uv_sets(&self) -> impl Iterator<Item = &'a [TextureCoordinate]> {
        self.uv_headers.iter().map(|header| {
            let uv_set_end = header.offset + header.count * std::mem::size_of::<TextureCoordinate>();
            bytemuck::cast_slice(&self.raw_uv_sets[header.offset..uv_set_end])            
        })
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MeshHeader {    
    pub primitive_header_offset: usize,
    pub primitive_header_count: usize,
    pub uv_header_offset: usize,
    pub uv_header_count: usize,
    pub texture_header_offset: usize,
    pub texture_header_count: usize,
    pub materials_offset: usize,
    pub materials_count: usize,
    pub samplers_offset: usize,        
    pub samplers_count: usize,        
    pub vertices_offset: usize,
    pub vertices_count: usize,
    pub indices_offset: usize,
    pub indices_count: usize,
    pub uv_sets_offset: usize,
    pub uv_sets_count: usize,
    pub texture_offset: usize,
    pub texture_size: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PrimitiveHeader {
    pub vertex_offset: usize,
    pub vertex_count: usize,
    pub index_offset: usize,
    pub index_count: usize,
    pub uv_header_offset: usize,
    pub uv_set_count: usize,
    pub material_index: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TexCoordHeader {
    offset: usize,
    count: usize,            
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureHeader {
    pub offset: usize,
    pub size: usize,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
}

pub struct PrimitiveHandle {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub uv_buffers: Vec<wgpu::Buffer>,
    pub num_elements: u32,
    pub material: usize,
}

// impl TransformUniform {
//     pub fn new() -> Self {
//         Self { transform: MAT_SWAP_YZ }
//     }

//     pub fn update(&mut self, transform: glam::Mat4) {
//         self.transform = transform.to_cols_array_2d()
//     }
// }

// pub struct Transform {
//     pub position: glam::Vec3,
//     pub rotation: glam::Quat,
//     pub scale: glam::Vec3,
//     pub index: usize,
// }

// impl Transform {
//     pub fn new(buffer: &mut TransformBuffer, context: &RenderContext) -> Self {
//         let matrix = MAT_SWAP_YZ;
//         let index = buffer.request_slot();

//         let offset = index * std::mem::size_of::<TransformUniform>();
//         context
//             .queue
//             .write_buffer(buffer.buffer(), offset as u64, bytemuck::bytes_of(&matrix));

//         Self {
//             matrix,
//             index,
//         }
//     }
// }

pub struct Mesh {
    pub label: Option<String>,
    pub primitives: Vec<PrimitiveHandle>,
    pub materials: Vec<MaterialHandle>,
}

impl Mesh {
    pub fn from_buffer(buffer: MeshBuffer, context: &RenderContext, label: Option<String>) -> Self {        
        let materials = buffer
            .iter_materials()
            .map(|material| {
                let view = material.base_color.unwrap();                
                
                let diffuse_texture =
                    Texture::from_view(&context.device, &context.queue, view, label.as_deref()).unwrap();
                let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: label.as_deref(),
                    layout: &context.texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                        },
                    ],
                });

                MaterialHandle {
                    diffuse_texture,
                    bind_group,
                }
            })
            .collect::<Vec<_>>();

        let primitives = buffer
            .iter_meshes()
            .map(|primitive| {
                let vertex_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: label.as_deref(),
                    contents: bytemuck::cast_slice(primitive.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: label.as_deref(),
                    contents: bytemuck::cast_slice(primitive.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                let uv_buffers = primitive.iter_uv_sets()
                    .map(|uv_set| {
                        context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: label.as_deref(),
                            contents: bytemuck::cast_slice(uv_set),
                            usage: wgpu::BufferUsages::VERTEX,
                        })
                    })
                    .collect::<Vec<_>>();

                PrimitiveHandle {
                    vertex_buffer,
                    index_buffer,
                    uv_buffers,
                    num_elements: primitive.indices.len() as u32,
                    material: primitive.material,
                }
            })
            .collect::<Vec<_>>();
                
        
        Self {
            primitives,
            materials,
            label,
        }
    }

    // pub fn translate(&mut self, translation: glam::Vec3) {
    //     self.transform.position += translation;
    //     self.transform.uniform.update(
    //         glam::Mat4::from_rotation_translation(self.transform.rotation, self.transform.position)
    //     );
    // }
}

pub struct MeshBuffer(Vec<u8>);

impl MeshBuffer {
    pub fn new(
        primitive_headers: Vec<PrimitiveHeader>,
        uv_headers: Vec<TexCoordHeader>,        
        texture_headers: Vec<TextureHeader>,        
        materials: Vec<Material>,
        samplers: Vec<Sampler>,
        vertices: Vec<MeshVertex>,
        indices: Vec<u32>,
        uv_sets: Vec<TextureCoordinate>,
        textures: Vec<u8>,
    ) -> Self {
        let mut builder = BlobBuilder::new();        
        let header_offset = builder.reserve::<MeshHeader>();

        let primitive_header_offset = builder.push_slice(&primitive_headers);
        let uv_header_offset = builder.push_slice(&uv_headers);
        let texture_header_offset = builder.push_slice(&texture_headers);
        let materials_offset = builder.push_slice(&materials);
        let samplers_offset = builder.push_slice(&samplers);
        let vertices_offset = builder.push_slice(&vertices);
        let indices_offset = builder.push_slice(&indices);
        let uv_sets_offset = builder.push_slice(&uv_sets);
        let texture_offset = builder.push_bytes(&textures);        

        let header = MeshHeader {
            primitive_header_offset,
            uv_header_offset,
            texture_header_offset,
            materials_offset,
            samplers_offset,
            vertices_offset,
            indices_offset,
            uv_sets_offset,
            texture_offset,
            primitive_header_count: primitive_headers.len(),
            uv_header_count: uv_headers.len(),
            texture_header_count: texture_headers.len(),
            materials_count: materials.len(),
            samplers_count: samplers.len(),
            vertices_count: vertices.len(),
            indices_count: indices.len(),
            uv_sets_count: uv_sets.len(),
            texture_size: textures.len(),
        };

        builder.write_at(header_offset, &header);
        Self(builder.finish())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    pub fn buffer(&self) -> &[u8] {
        &self.0
    }
    
    pub fn get_slice<T: Pod>(&self, offset: usize, count: usize) -> &[T] {
        let end = offset + count * std::mem::size_of::<T>();
        bytemuck::cast_slice(&self.0[offset..end])
    }

    pub fn get_raw_slice<T: Pod>(&self, offset: usize, count: usize) -> &[u8] {
        let end = offset + count * std::mem::size_of::<T>();
        bytemuck::cast_slice(&self.0[offset..end])        
    }

    pub fn get_slice_of<T: Pod>(buffer: &[u8], offset: usize, count: usize) -> &[T] {
        let end = offset + count * std::mem::size_of::<T>();
        bytemuck::cast_slice(&buffer[offset..end])        
    }

    pub fn iter_meshes(&self) -> impl Iterator<Item = PrimitiveView<'_>> {
        let mesh_header: &MeshHeader = bytemuck::from_bytes(&self.0[..std::mem::size_of::<MeshHeader>()]);
        let raw_vertices = self.get_raw_slice::<MeshVertex>(mesh_header.vertices_offset, mesh_header.vertices_count);
        let raw_indices= self.get_raw_slice::<u32>(mesh_header.indices_offset, mesh_header.indices_count);
        let raw_uv_headers= self.get_raw_slice::<TexCoordHeader>(mesh_header.uv_header_offset, mesh_header.uv_header_count);
        let raw_uv_sets= self.get_raw_slice::<TextureCoordinate>(mesh_header.uv_sets_offset, mesh_header.uv_sets_count);

        self.get_slice::<PrimitiveHeader>(mesh_header.primitive_header_offset, mesh_header.primitive_header_count)
            .iter()
            .map(|primitive_header| {            
                let vertices: &[MeshVertex] = Self::get_slice_of(raw_vertices, primitive_header.vertex_offset, primitive_header.vertex_count);
                let indices: &[u32] = Self::get_slice_of(raw_indices, primitive_header.index_offset, primitive_header.index_count);
                let uv_headers: &[TexCoordHeader] = Self::get_slice_of(raw_uv_headers, primitive_header.uv_header_offset, primitive_header.uv_set_count);                             

                PrimitiveView {
                    vertices,
                    indices,
                    material: primitive_header.material_index,
                    uv_headers,
                    raw_uv_sets,
                }
            })
    }

    pub fn iter_materials(&self) -> impl Iterator<Item = MaterialView<'_>> {
        let mesh_header: &MeshHeader = bytemuck::from_bytes(&self.0[..std::mem::size_of::<MeshHeader>()]);
        let texture_headers: &[TextureHeader] = self.get_slice(mesh_header.texture_header_offset, mesh_header.texture_header_count);
        let materials: &[Material] = self.get_slice(mesh_header.materials_offset, mesh_header.materials_count); 
        let samplers: &[Sampler] = self.get_slice(mesh_header.samplers_offset, mesh_header.samplers_count); 
        let raw_textures = self.get_slice(mesh_header.texture_offset, mesh_header.texture_size);
        
        let create_texture_view = |texture_slot: Option<TextureSlot>| {
            if let Some(slot) = texture_slot {
                let header = texture_headers.get(slot.texture_index as usize).unwrap();            
                let texture = &raw_textures[header.offset..header.offset + header.size];
                let sampler = samplers.get(slot.sampler_index as usize).copied().unwrap_or_default();
                let view = TextureView {
                    format: header.format,
                    width: header.width,
                    height: header.height,
                    uv_index: slot.uv_index,
                    texture,
                    sampler,                
                };

                Some(view)      
            } else {
                None
            }
        };

        materials.iter()
            .map(move |material| {
                MaterialView {
                    base_color: create_texture_view(material.base_color),
                    metallic_roughness: create_texture_view(material.metallic_roughness),
                    normal: create_texture_view(material.normal),
                    occlusion: create_texture_view(material.occlusion),
                    emissive: create_texture_view(material.emissive),
                    base_color_factor: material.base_color_factor,
                    emissive_factor: material.emissive_factor,
                    metallic_factor: material.metallic_factor,
                    roughness_factor: material.roughness_factor,
                    occlusion_strength: material.occlusion_strength,
                    normal_scale: material.normal_scale,
                    alpha_cutoff: material.alpha_cutoff, 
                    alpha_mode: material.alpha_mode,
                    double_sided: material.double_sided,
                }                
            })
    }

    pub async fn from_gltf(path: &ResourcePath) -> anyhow::Result<Self> {
        let data = path.load_binary().await?;
        let (gltf, buffers, images) = gltf::import_slice(data)?;
        
        let mut textures = Vec::new();
        let mut texture_headers = Vec::new();
        let mut samplers = Vec::new();
        let mut materials = Vec::new();

        for material in gltf.materials() {
            materials.push(Material::from_gltf(&material));
        }
       
        for sampler in gltf.samplers() {            
            samplers.push(Sampler::from_gltf(&sampler));
        }        

        for image in images {
            let header = TextureHeader {
                offset: textures.len(),
                size: image.pixels.len(),
                width: image.width,
                height: image.height,
                format: TextureFormat::from_gltf(&image.format),
            };

            texture_headers.push(header);
            textures.extend(image.pixels);
        }
    
        let scene = gltf.default_scene().unwrap_or_else(|| gltf.scenes().next().unwrap());

        let mut primitive_headers = Vec::new();        
        let mut uv_headers = Vec::new();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut uv_sets = Vec::new();

        for node in scene.nodes() {
            if let Some(mesh) = node.mesh() {
                for primitive in mesh.primitives() {
                    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                    let primitive_indices = reader.read_indices().unwrap().into_u32().collect::<Vec<_>>();
                    let positions = reader.read_positions().unwrap();
                    let normals = reader.read_normals().unwrap();
                    
                    let primitive_vertices = positions
                        .zip(normals)
                        .map(|(position, normal)| MeshVertex {
                            position, normal
                        })
                        .collect::<Vec<_>>();

                    let mut primitive_uv_headers = Vec::new();
                    for set_index in 0..6 {
                        if let Some(uv_reader) = reader.read_tex_coords(set_index) {
                            let uv_set = uv_reader.into_f32().map(|[u, v]| TextureCoordinate([u, 1.0 - v])).collect::<Vec<_>>();                            
                            let header = TexCoordHeader {
                                offset: std::mem::size_of::<TextureCoordinate>() * uv_sets.len(),
                                count: uv_set.len(),
                            };

                            primitive_uv_headers.push(header);
                            uv_sets.extend(uv_set);
                        } else {
                            break;
                        }
                    }                     

                    let header = PrimitiveHeader {
                        vertex_offset: std::mem::size_of::<MeshVertex>() * vertices.len(),
                        vertex_count: primitive_vertices.len(),
                        index_offset: std::mem::size_of::<u32>() * indices.len(),
                        index_count: primitive_indices.len(),
                        uv_header_offset: std::mem::size_of::<TexCoordHeader>() * uv_headers.len(),
                        uv_set_count: primitive_uv_headers.len(),
                        material_index: primitive.material().index().unwrap_or(0),
                    };

                    primitive_headers.push(header);
                    uv_headers.extend(primitive_uv_headers);
                    vertices.extend(primitive_vertices);
                    indices.extend(primitive_indices);
                }
            }
        }

        Ok(Self::new(
            primitive_headers, 
            uv_headers, 
            texture_headers,
            materials,
            samplers,
            vertices, 
            indices,
            uv_sets, 
            textures,
        ))
    }

    pub async fn from_obj(path: &ResourcePath) -> anyhow::Result<Self> {
        let text = path.load_string().await?;
        let cursor = Cursor::new(text);
        let mut reader = BufReader::new(cursor);

        let (models, obj_materials) = tobj::load_obj_buf_async(
            &mut reader,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
            |p| async move {
                let material_text = path.create_relative(&p).load_string().await.unwrap();
                tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(material_text)))
            },
        )
        .await?;

        let mut textures = Vec::new();
        let mut texture_headers = Vec::new();
        let mut samplers = Vec::new();
        let mut materials = Vec::new();

        for material in &obj_materials? {
            if let Some(filename) = &material.diffuse_texture {
                let texture_path = path.create_relative(&filename);
                let texture = texture_path.load_binary().await?;
                let image = image::load_from_memory(&texture)?.to_rgba8();
                let buffer = image.as_bytes();
                let header = TextureHeader {
                    offset: textures.len(),
                    size: buffer.len(),
                    width: image.width(),
                    height: image.height(),
                    format: TextureFormat::RGBA8,
                };

                let new_material = Material::from_obj(&material);

                materials.push(new_material);
                texture_headers.push(header);
                textures.extend(buffer);
            }
        }

        let (primitive_headers, uv_headers, vertices, indices, uv_sets) =
            models
                .into_iter()
                .fold((Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()), |accumulator, model| {
                    let (mut primitive_headers, mut uv_headers, mut vertices, mut indices, mut uv_sets) = accumulator;
                    let (model_vertices, tex_coords): (Vec<_>, Vec<_>) = (0..model.mesh.positions.len() / 3).map(|i| {
                        if model.mesh.normals.is_empty() {
                            (
                                MeshVertex {
                                    position: [
                                        model.mesh.positions[i * 3],
                                        model.mesh.positions[i * 3 + 1],
                                        model.mesh.positions[i * 3 + 2],
                                    ],                                                                
                                    normal: [0.0, 0.0, 0.0],
                                },
                                TextureCoordinate([model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]])
                            )
                        } else {
                            (
                                MeshVertex {
                                    position: [
                                        model.mesh.positions[i * 3],
                                        model.mesh.positions[i * 3 + 1],
                                        model.mesh.positions[i * 3 + 2],
                                    ],
                                    normal: [
                                        model.mesh.normals[i * 3],
                                        model.mesh.normals[i * 3 + 1],
                                        model.mesh.normals[i * 3 + 2],
                                    ],
                                },
                                TextureCoordinate([model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]])
                            )
                        }
                    }).unzip();

                    let uv_header = TexCoordHeader {
                        offset: 0,
                        count: tex_coords.len(),
                    };                

                    let header = PrimitiveHeader {
                        vertex_offset: std::mem::size_of::<MeshVertex>() * vertices.len(),
                        vertex_count: model_vertices.len(),
                        index_offset: std::mem::size_of::<u32>() * indices.len(),
                        index_count: model.mesh.indices.len(),
                        uv_header_offset: std::mem::size_of::<TexCoordHeader>() * uv_headers.len(),
                        uv_set_count: 1,
                        material_index: model.mesh.material_id.unwrap_or(0),
                    };

                    primitive_headers.push(header);
                    uv_headers.push(uv_header);
                    vertices.extend(model_vertices);
                    indices.extend(model.mesh.indices);
                    uv_sets.extend(tex_coords);

                    (primitive_headers, uv_headers, vertices, indices, uv_sets)
                });

        Ok(Self::new(
            primitive_headers, 
            uv_headers, 
            texture_headers,
            materials,
            samplers,
            vertices, 
            indices,
            uv_sets, 
            textures,
        ))
    }
}
