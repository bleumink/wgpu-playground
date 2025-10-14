use std::{
    io::{BufReader, Cursor},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};
use gltf::{image::Format as GltfImageFormat, mesh::util::ReadTexCoords, Gltf};
use uuid::Uuid;
use wgpu::util::DeviceExt;

use crate::{asset::ResourcePath, context::RenderContext, renderer::TransformBuffer, texture::{Texture, TextureFormat}, vertex::Vertex};

const MAT_SWAP_YZ: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub trait DrawModel<'a> {
    // fn draw_mesh(&mut self, mesh: &'a Mesh, material: &'a Material);
    fn draw_mesh_instanced(&mut self, mesh: &'a Mesh, material: &'a Material, instances: Range<u32>);
    // fn draw_model(&mut self, model: &'a Model);
    fn draw_model_instanced(&mut self, model: &'a Model, instances: Range<u32>);
}

impl<'a, 'b> DrawModel<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    // fn draw_mesh(&mut self, mesh: &'b Mesh, material: &'b Material) {
    //     self.draw_mesh_instanced(mesh, material, 0..1);
    // }

    fn draw_mesh_instanced(&mut self, mesh: &'b Mesh, material: &'b Material, instances: Range<u32>) {
        self.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        self.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        self.set_bind_group(0, &material.bind_group, &[]);

        self.draw_indexed(0..mesh.num_elements, 0, instances);
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

    fn draw_model_instanced(&mut self, model: &'b Model, instances: Range<u32>) {
        for mesh in &model.meshes {
            let material = &model.materials[mesh.material];
            self.draw_mesh_instanced(mesh, material, instances.clone());
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex for ModelVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelVertex>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub struct ModelView<'a> {
    pub meshes: &'a [MeshView<'a>],
    pub materials: &'a [MaterialView<'a>],
}

pub struct MeshView<'a> {
    pub vertices: &'a [ModelVertex],
    pub indices: &'a [u32],
    pub material: usize,
}

pub struct MaterialView<'a> {
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub texture: &'a [u8],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ModelHeader {
    pub mesh_header_offset: usize,
    pub material_header_offset: usize,
    pub vertices_offset: usize,
    pub indices_offset: usize,
    pub texture_offset: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MeshHeader {
    pub vertex_offset: usize,
    pub vertex_count: usize,
    pub index_offset: usize,
    pub index_count: usize,
    pub material_index: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialHeader {
    pub offset: usize,
    pub size: usize,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
}

pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_elements: u32,
    pub material: usize,
}

pub struct Material {
    pub diffuse_texture: Texture,
    pub bind_group: wgpu::BindGroup,
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

pub struct Model {
    pub label: Option<String>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}

impl Model {
    pub fn from_buffer(buffer: ModelBuffer, context: &RenderContext, label: Option<String>) -> Self {
        let materials = buffer
            .iter_materials()
            .map(|material| {
                let diffuse_texture = Texture::from_view(
                    &context.device,
                    &context.queue,
                    material,
                    label.as_deref(),
                )
                .unwrap();
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

                Material {
                    diffuse_texture,
                    bind_group,
                }
            })
            .collect::<Vec<_>>();

        let meshes = buffer
            .iter_meshes()
            .map(|mesh| {
                let vertex_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: label.as_deref(),
                    contents: bytemuck::cast_slice(mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: label.as_deref(),
                    contents: bytemuck::cast_slice(mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                Mesh {
                    vertex_buffer,
                    index_buffer,
                    num_elements: mesh.indices.len() as u32,
                    material: mesh.material,
                }
            })
            .collect::<Vec<_>>();

        Self {
            meshes,
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

pub struct ModelBuffer(Vec<u8>);

impl ModelBuffer {
    pub fn new(
        mesh_headers: Vec<MeshHeader>,
        material_headers: Vec<MaterialHeader>,
        vertices: Vec<ModelVertex>,
        indices: Vec<u32>,
        textures: Vec<u8>,
    ) -> Self {
        let mesh_header_offset = std::mem::size_of::<ModelHeader>();
        let material_header_offset = mesh_header_offset + std::mem::size_of::<MeshHeader>() * mesh_headers.len();
        let vertices_offset = material_header_offset + std::mem::size_of::<MaterialHeader>() * material_headers.len();
        let indices_offset = vertices_offset + std::mem::size_of::<ModelVertex>() * vertices.len();
        let texture_offset = indices_offset + std::mem::size_of::<u32>() * indices.len();
        let header = ModelHeader {
            mesh_header_offset,
            material_header_offset,
            vertices_offset,
            indices_offset,
            texture_offset,
        };

        let capacity = texture_offset + textures.len();
        let mut buffer = Vec::new();

        buffer.reserve_exact(capacity);
        buffer.extend_from_slice(bytemuck::bytes_of(&header));
        buffer.extend_from_slice(bytemuck::cast_slice(&mesh_headers));
        buffer.extend_from_slice(bytemuck::cast_slice(&material_headers));
        buffer.extend_from_slice(bytemuck::cast_slice(&vertices));
        buffer.extend_from_slice(bytemuck::cast_slice(&indices));
        buffer.extend_from_slice(bytemuck::cast_slice(&textures));

        Self(buffer)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    pub fn buffer(&self) -> &[u8] {
        &self.0
    }

    pub fn iter_meshes(&self) -> impl Iterator<Item = MeshView<'_>> {
        let model_header = bytemuck::from_bytes::<ModelHeader>(&self.0[..std::mem::size_of::<ModelHeader>()]);
        let raw_vertices = &self.0[model_header.vertices_offset..model_header.indices_offset];
        let raw_indices = &self.0[model_header.indices_offset..model_header.texture_offset];

        self.0[model_header.mesh_header_offset..model_header.material_header_offset]
            .chunks_exact(std::mem::size_of::<MeshHeader>())
            .map(|chunk| {
                let header = bytemuck::from_bytes::<MeshHeader>(chunk);

                let vertex_end = header.vertex_offset + header.vertex_count * std::mem::size_of::<ModelVertex>();
                let vertices: &[ModelVertex] = bytemuck::cast_slice(&raw_vertices[header.vertex_offset..vertex_end]);

                let index_end = header.index_offset + header.index_count * std::mem::size_of::<u32>();
                let indices: &[u32] = bytemuck::cast_slice(&raw_indices[header.index_offset..index_end]);

                MeshView {
                    vertices,
                    indices: indices,
                    material: header.material_index,
                }
            })
    }

    pub fn iter_materials(&self) -> impl Iterator<Item = MaterialView<'_>> {
        let model_header = bytemuck::from_bytes::<ModelHeader>(&self.0[..std::mem::size_of::<ModelHeader>()]);
        let raw_textures = &self.0[model_header.texture_offset..];

        self.0[model_header.material_header_offset..model_header.vertices_offset]
            .chunks_exact(std::mem::size_of::<MaterialHeader>())
            .map(|chunk| {
                let header = bytemuck::from_bytes::<MaterialHeader>(chunk);
                let texture_end = header.offset + header.size;
                let texture = &raw_textures[header.offset..texture_end];

                MaterialView { 
                    format: header.format,
                    width: header.width,
                    height: header.height,
                    texture,
                 }
            })
    }

    pub async fn from_gltf(path: &ResourcePath) -> anyhow::Result<Self> {
        let data = path.load_binary().await?;        
        let (gltf, buffers, images) = gltf::import_slice(data)?;
        
        let mut material_headers = Vec::new();
        let mut textures = Vec::new();
        for image in images {
            let header = MaterialHeader {
                offset: textures.len(),
                size: image.pixels.len(),
                width: image.width,
                height: image.height,
                format: TextureFormat::from_gltf(image.format),
            };
            
            material_headers.push(header);
            textures.extend(image.pixels);
        }

        let scene = match gltf.default_scene() {
            Some(scene) => scene,
            None => gltf.scenes().next().unwrap()
        };

        let (mesh_headers, vertices, indices) = scene.nodes().fold((Vec::new(), Vec::new(), Vec::new()), |accumulator, node| {
            let (mut mesh_headers, mut vertices, mut indices) = accumulator;
            if let Some(mesh) = node.mesh() {
                for primitive in mesh.primitives() {
                    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                    let mesh_indices = reader.read_indices().unwrap().into_u32().collect::<Vec<_>>();
                    let positions = reader.read_positions().unwrap();
                    let normals = reader.read_normals().unwrap();
                    let tex_coords = reader.read_tex_coords(0)
                        .map(|t| t.into_f32().map(|[u, v]| [u, 1.0 - v])).unwrap();
                    let mesh_vertices = positions
                        .zip(normals)
                        .zip(tex_coords)
                        .map(|((position, normal), tex_coords)| ModelVertex {
                            position, tex_coords, normal,
                        })
                        .collect::<Vec<_>>();
                    
                    // log::info!("{:?}", gltf.textures());
                    // log::info!("{:?}", gltf.samplers());

                    let header = MeshHeader {
                        vertex_offset: std::mem::size_of::<ModelVertex>() * vertices.len(),
                        vertex_count: mesh_vertices.len(),
                        index_offset: std::mem::size_of::<u32>() * indices.len(),
                        index_count: mesh_indices.len(),
                        material_index: primitive.material().index().unwrap_or(0),
                    };

                    mesh_headers.push(header);
                    vertices.extend(mesh_vertices);
                    indices.extend(mesh_indices);
                }
            }

            (mesh_headers, vertices, indices)
        });

        Ok(Self::new(mesh_headers, material_headers, vertices, indices, textures))
    }

    pub async fn from_obj(path: &ResourcePath) -> anyhow::Result<Self> {
        let text = path.load_string().await?;
        let cursor = Cursor::new(text);
        let mut reader = BufReader::new(cursor);

        let (models, materials) = tobj::load_obj_buf_async(
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

        let mut material_headers = Vec::new();
        let mut textures = Vec::new();

        for material in materials? {
            if let Some(filename) = material.diffuse_texture {
                let texture_path = path.create_relative(&filename);
                let texture = texture_path.load_binary().await?;
                let header = MaterialHeader {
                    offset: textures.len(),
                    size: texture.len(),
                    width: 0,
                    height: 0,
                    format: TextureFormat::RGBA8,
                };

                material_headers.push(header);
                textures.extend(texture);
            }
        }

        let (mesh_headers, vertices, indices) =
            models
                .into_iter()
                .fold((Vec::new(), Vec::new(), Vec::new()), |accumulator, model| {
                    let (mut mesh_headers, mut vertices, mut indices) = accumulator;
                    let model_vertices = (0..model.mesh.positions.len() / 3).map(|i| {
                        if model.mesh.normals.is_empty() {
                            ModelVertex {
                                position: [
                                    model.mesh.positions[i * 3],
                                    model.mesh.positions[i * 3 + 1],
                                    model.mesh.positions[i * 3 + 2],
                                ],
                                tex_coords: [model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]],
                                normal: [0.0, 0.0, 0.0],
                            }
                        } else {
                            ModelVertex {
                                position: [
                                    model.mesh.positions[i * 3],
                                    model.mesh.positions[i * 3 + 1],
                                    model.mesh.positions[i * 3 + 2],
                                ],
                                tex_coords: [model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]],
                                normal: [
                                    model.mesh.normals[i * 3],
                                    model.mesh.normals[i * 3 + 1],
                                    model.mesh.normals[i * 3 + 2],
                                ],
                            }
                        }
                    });

                    let header = MeshHeader {
                        vertex_offset: std::mem::size_of::<ModelVertex>() * vertices.len(),
                        vertex_count: model_vertices.len(),
                        index_offset: std::mem::size_of::<u32>() * indices.len(),
                        index_count: model.mesh.indices.len(),
                        material_index: model.mesh.material_id.unwrap_or(0),
                    };

                    mesh_headers.push(header);
                    vertices.extend(model_vertices);
                    indices.extend(model.mesh.indices);

                    (mesh_headers, vertices, indices)
                });

        Ok(Self::new(mesh_headers, material_headers, vertices, indices, textures))
    }
}
