use std::{
    io::{BufReader, Cursor},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};
use glam::Vec4Swizzles;
use gltf::json::extensions::texture;
use image::EncodableLayout;
use wgpu::util::DeviceExt;

use crate::renderer::{
    asset::ResourcePath,
    binary::BlobBuilder,
    context::RenderContext,
    material::{Material, MaterialUniform, MaterialView, RawMaterial, TextureSlot},
    texture::{Sampler, Texture, TextureFormat, TextureView},
    vertex::Vertex,
};

pub trait DrawMesh<'a> {
    fn draw_primitive_instanced(&mut self, primitive: &'a Primitive, material: &'a Material, instances: Range<u32>);
    fn draw_mesh_instanced(&mut self, mesh: &'a Mesh, material: &'a [Material], instances: Range<u32>);
}

impl<'a, 'b> DrawMesh<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    fn draw_primitive_instanced(&mut self, primitive: &'b Primitive, material: &'b Material, instances: Range<u32>) {
        self.set_vertex_buffer(0, primitive.vertex_buffer.slice(..));
        self.set_index_buffer(primitive.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        primitive
            .uv_buffers
            .iter()
            .enumerate()
            .for_each(|(index, uv_set)| self.set_vertex_buffer(1 + index as u32, uv_set.slice(..)));

        self.set_bind_group(0, &material.bind_group, &[]);
        self.draw_indexed(0..primitive.num_elements, 0, instances);
    }

    fn draw_mesh_instanced(&mut self, mesh: &'b Mesh, materials: &'b [Material], instances: Range<u32>) {
        for primitive in &mesh.primitives {
            let material = &materials[primitive.material_index];
            self.draw_primitive_instanced(primitive, material, instances.clone());
        }
    }
}

fn index_to_position(positions: &[glam::Vec3], indices: &[u32]) -> [glam::Vec3; 3] {
    let v0 = positions[indices[0] as usize];
    let v1 = positions[indices[1] as usize];
    let v2 = positions[indices[2] as usize];

    [v0, v1, v2]
}

fn calculate_normals(positions: &[glam::Vec3], indices: &[u32]) -> Vec<glam::Vec3> {
    indices
        .chunks_exact(3)
        .map(|index| {
            let [v0, v1, v2] = index_to_position(positions, index);
            (v1 - v0).cross(v2 - v0).normalize_or_zero()
        })
        .collect()
}

fn calculate_tangents(
    positions: &[glam::Vec3],
    normals: &[glam::Vec3],
    indices: &[u32],
    uvs: &[TextureCoordinate],
) -> Vec<glam::Vec4> {
    let mut tangents = vec![glam::Vec3::ZERO; positions.len()];
    let mut bitangents = vec![glam::Vec3::ZERO; positions.len()];

    for index in indices.chunks_exact(3) {
        let [v0, v1, v2] = index_to_position(positions, index);

        let uv0 = uvs[index[0] as usize].to_vec();
        let uv1 = uvs[index[1] as usize].to_vec();
        let uv2 = uvs[index[2] as usize].to_vec();

        let delta_pos1 = v1 - v0;
        let delta_pos2 = v2 - v0;

        let delta_uv1 = uv1 - uv0;
        let delta_uv2 = uv2 - uv0;

        let r = 1.0 / (delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x);
        let tangent = (delta_pos1 * delta_uv2.y - delta_pos2 * delta_uv1.y) * r;
        let bitangent = (delta_pos2 * delta_uv1.x - delta_pos1 * delta_uv2.x) * -r;

        tangents[index[0] as usize] += tangent;
        tangents[index[1] as usize] += tangent;
        tangents[index[2] as usize] += tangent;

        bitangents[index[0] as usize] += bitangent;
        bitangents[index[1] as usize] += bitangent;
        bitangents[index[2] as usize] += bitangent;
    }

    tangents
        .into_iter()
        .zip(bitangents)
        .zip(normals)
        .map(|((tangent, bitangent), normal)| {
            let t = (tangent - normal * tangent.dot(*normal)).normalize_or_zero();
            let w = if normal.cross(t).dot(bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            };
            glam::Vec4::new(t.x, t.y, t.z, w)
        })
        .collect()
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
}

impl MeshVertex {
    pub fn new(position: glam::Vec3, normal: glam::Vec3, tangent: glam::Vec4) -> Self {
        Self {
            position: [position.x, position.y, position.z],
            normal: [normal.x, normal.y, normal.z],
            tangent: tangent.to_array(),
        }
    }
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
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureCoordinate([f32; 2]);
impl TextureCoordinate {
    fn new(uv_coordinates: [f32; 2]) -> Self {
        Self(uv_coordinates)
    }

    fn from_slice(uv_coordinates: &[f32]) -> Self {
        Self([uv_coordinates[0], uv_coordinates[1]])
    }
}

impl Vertex for TextureCoordinate {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

impl Default for TextureCoordinate {
    fn default() -> Self {
        Self([0.0, 0.0])
    }
}

impl TextureCoordinate {
    pub fn to_vec(&self) -> glam::Vec2 {
        glam::Vec2::from_array(self.0)
    }
}

pub struct PrimitiveView<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
    pub material_index: usize,
    uv_headers: &'a [TexCoordHeader],
    raw_uv_sets: &'a [u8],
}

impl<'a> PrimitiveView<'a> {
    pub fn get_uv_set(&self, index: usize) -> Option<&'a [TextureCoordinate]> {
        self.uv_headers.get(index).and_then(|header| {
            let uv_set_end = header.offset + header.count * std::mem::size_of::<TextureCoordinate>();
            let slice = &self.raw_uv_sets[header.offset..uv_set_end];
            Some(bytemuck::cast_slice(slice))
        })
    }

    pub fn iter_uv_sets(&self) -> impl Iterator<Item = &'a [TextureCoordinate]> {
        self.uv_headers.iter().map(|header| {
            let uv_set_end = header.offset + header.count * std::mem::size_of::<TextureCoordinate>();
            bytemuck::cast_slice(&self.raw_uv_sets[header.offset..uv_set_end])
        })
    }

    pub fn to_owned(self, context: &RenderContext, label: Option<&str>) -> Primitive {
        Primitive::from_view(self, context, label.as_deref())
    }
}

pub struct NodeView<'a> {
    pub transform: glam::Mat4,
    pub primitives: Vec<PrimitiveView<'a>>,
}

impl NodeView<'_> {
    pub fn to_owned(self, context: &RenderContext, label: Option<&str>) -> Node {
        Node::from_view(self, context, label)
    }
}

#[derive(Debug)]
pub struct Node {
    pub transform: glam::Mat4,
    pub mesh: Mesh,
}

impl Node {
    pub fn from_view(view: NodeView, context: &RenderContext, label: Option<&str>) -> Self {
        let primitives = view
            .primitives
            .into_iter()
            .map(|primitive| primitive.to_owned(context, label))
            .collect();

        Self {
            transform: view.transform,
            mesh: Mesh { primitives },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub primitives: Vec<Primitive>,
}

impl Mesh {
    pub fn unit_cube(context: &RenderContext) -> Self {
        let (vertices, indices, uv_set) = unit_cube();

        let vertex_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Unit cube vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Unit cube indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let dummy_uv_set = [TextureCoordinate::default()];
        let uv_sets = vec![uv_set.as_slice()];
        let uv_buffers = (0..6)
            .map(|uv_index| {
                let uv = uv_sets.get(uv_index).copied().unwrap_or(dummy_uv_set.as_slice());
                context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Unit cube UV set"),
                    contents: bytemuck::cast_slice(&uv),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect::<Vec<_>>();

        let primitive = Primitive {
            vertex_buffer,
            index_buffer,
            uv_buffers,
            num_elements: indices.len() as u32,
            material_index: 0,
        };

        Self {
            primitives: vec![primitive],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SceneHeader {
    pub node_header_offset: usize,
    pub node_header_count: usize,
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
pub struct NodeHeader {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub primitive_header_offset: usize,
    pub primitive_count: usize,
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

#[derive(Clone, Debug)]
pub struct Primitive {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub uv_buffers: Vec<wgpu::Buffer>,
    pub num_elements: u32,
    pub material_index: usize,
}

impl Primitive {
    pub fn from_view(view: PrimitiveView, context: &RenderContext, label: Option<&str>) -> Self {
        let vertex_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: label.as_deref(),
            contents: bytemuck::cast_slice(view.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: label.as_deref(),
            contents: bytemuck::cast_slice(view.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let dummy_uv_set = [TextureCoordinate::default()];
        let uv_buffers = (0..6)
            .map(|uv_index| {
                let uv_set = view.get_uv_set(uv_index).unwrap_or(&dummy_uv_set);
                context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: label.as_deref(),
                    contents: bytemuck::cast_slice(&uv_set),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect::<Vec<_>>();

        Self {
            vertex_buffer,
            index_buffer,
            uv_buffers,
            num_elements: view.indices.len() as u32,
            material_index: view.material_index,
        }
    }
}

#[derive(Debug)]
pub struct Scene {
    pub label: Option<String>,
    pub nodes: Vec<Node>,
    pub materials: Vec<Material>,
}

impl Scene {
    pub fn from_buffer(buffer: SceneBuffer, context: &RenderContext, label: Option<String>) -> Self {
        let materials = buffer
            .iter_materials()
            .map(|material| Material::new(material, label.as_deref(), context))
            .collect::<Vec<_>>();

        let nodes = buffer
            .iter_nodes()
            .map(|node| node.to_owned(context, label.as_deref()))
            .collect();

        Self {
            nodes,
            materials,
            label,
        }
    }
}

pub struct SceneBuffer(Vec<u8>);
impl SceneBuffer {
    pub fn new(
        node_headers: Vec<NodeHeader>,
        primitive_headers: Vec<PrimitiveHeader>,
        uv_headers: Vec<TexCoordHeader>,
        texture_headers: Vec<TextureHeader>,
        materials: Vec<RawMaterial>,
        samplers: Vec<Sampler>,
        vertices: Vec<MeshVertex>,
        indices: Vec<u32>,
        uv_sets: Vec<TextureCoordinate>,
        textures: Vec<u8>,
    ) -> Self {
        let mut builder = BlobBuilder::new();
        let header_offset = builder.reserve::<SceneHeader>();

        let node_header_offset = builder.push_slice(&node_headers);
        let primitive_header_offset = builder.push_slice(&primitive_headers);
        let uv_header_offset = builder.push_slice(&uv_headers);
        let texture_header_offset = builder.push_slice(&texture_headers);
        let materials_offset = builder.push_slice(&materials);
        let samplers_offset = builder.push_slice(&samplers);
        let vertices_offset = builder.push_slice(&vertices);
        let indices_offset = builder.push_slice(&indices);
        let uv_sets_offset = builder.push_slice(&uv_sets);
        let texture_offset = builder.push_bytes(&textures);

        let header = SceneHeader {
            node_header_offset,
            primitive_header_offset,
            uv_header_offset,
            texture_header_offset,
            materials_offset,
            samplers_offset,
            vertices_offset,
            indices_offset,
            uv_sets_offset,
            texture_offset,
            node_header_count: node_headers.len(),
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

    pub fn slice<T: Pod>(&self, offset: usize, count: usize) -> &[T] {
        Self::slice_as(&self.0, offset, count)
    }

    pub fn slice_raw<T: Pod>(&self, offset: usize, count: usize) -> &[u8] {
        let end = offset + count * std::mem::size_of::<T>();
        bytemuck::cast_slice(&self.0[offset..end])
    }

    pub fn slice_as<T: Pod>(buffer: &[u8], offset: usize, count: usize) -> &[T] {
        let end = offset + count * std::mem::size_of::<T>();
        bytemuck::cast_slice(&buffer[offset..end])
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = NodeView<'_>> {
        let scene_header: &SceneHeader = bytemuck::from_bytes(&self.0[..std::mem::size_of::<SceneHeader>()]);

        let raw_primitive_headers = self.slice_raw::<PrimitiveHeader>(
            scene_header.primitive_header_offset,
            scene_header.primitive_header_count,
        );
        let raw_vertices = self.slice_raw::<MeshVertex>(scene_header.vertices_offset, scene_header.vertices_count);
        let raw_indices = self.slice_raw::<u32>(scene_header.indices_offset, scene_header.indices_count);
        let raw_uv_headers =
            self.slice_raw::<TexCoordHeader>(scene_header.uv_header_offset, scene_header.uv_header_count);
        let raw_uv_sets = self.slice_raw::<TextureCoordinate>(scene_header.uv_sets_offset, scene_header.uv_sets_count);

        self.slice::<NodeHeader>(scene_header.node_header_offset, scene_header.node_header_count)
            .iter()
            .map(|node_header| {
                let transform = glam::Mat4::from_scale_rotation_translation(
                    glam::Vec3::from_slice(&node_header.scale),
                    glam::Quat::from_slice(&node_header.rotation),
                    glam::Vec3::from_slice(&node_header.position),
                );

                let primitive_headers: &[PrimitiveHeader] = Self::slice_as(
                    raw_primitive_headers,
                    node_header.primitive_header_offset,
                    node_header.primitive_count,
                );
                let primitives = primitive_headers
                    .iter()
                    .map(|primitive_header| {
                        let vertices: &[MeshVertex] = Self::slice_as(
                            raw_vertices,
                            primitive_header.vertex_offset,
                            primitive_header.vertex_count,
                        );
                        let indices: &[u32] =
                            Self::slice_as(raw_indices, primitive_header.index_offset, primitive_header.index_count);
                        let uv_headers: &[TexCoordHeader] = Self::slice_as(
                            raw_uv_headers,
                            primitive_header.uv_header_offset,
                            primitive_header.uv_set_count,
                        );

                        PrimitiveView {
                            vertices,
                            indices,
                            material_index: primitive_header.material_index,
                            uv_headers,
                            raw_uv_sets,
                        }
                    })
                    .collect();

                NodeView { primitives, transform }
            })
    }

    pub fn iter_materials(&self) -> impl Iterator<Item = MaterialView<'_>> {
        let scene_header: &SceneHeader = bytemuck::from_bytes(&self.0[..std::mem::size_of::<SceneHeader>()]);
        let texture_headers: &[TextureHeader] =
            self.slice(scene_header.texture_header_offset, scene_header.texture_header_count);
        let materials: &[RawMaterial] = self.slice(scene_header.materials_offset, scene_header.materials_count);
        let samplers: &[Sampler] = self.slice(scene_header.samplers_offset, scene_header.samplers_count);
        let raw_textures = self.slice(scene_header.texture_offset, scene_header.texture_size);

        let create_texture_view = |texture_slot: Option<TextureSlot>, is_srgb: bool| {
            texture_slot.and_then(|slot| {
                let header = texture_headers[slot.texture_index as usize];
                let texture = &raw_textures[header.offset..header.offset + header.size];
                let sampler = samplers.get(slot.sampler_index as usize).copied().unwrap_or_default();
                let view = TextureView {
                    format: header.format,
                    width: header.width,
                    height: header.height,
                    uv_index: slot.uv_index,
                    texture,
                    sampler,
                    is_srgb,
                };

                Some(view)
            })
        };

        materials.iter().map(move |material| MaterialView {
            base_color: create_texture_view(material.base_color, true),
            metallic_roughness: create_texture_view(material.metallic_roughness, false),
            normal: create_texture_view(material.normal, false),
            occlusion: create_texture_view(material.occlusion, false),
            emissive: create_texture_view(material.emissive, true),
            base_color_factor: material.base_color_factor,
            emissive_factor: material.emissive_factor,
            metallic_factor: material.metallic_factor,
            roughness_factor: material.roughness_factor,
            occlusion_strength: material.occlusion_strength,
            normal_scale: material.normal_scale,
            alpha_cutoff: material.alpha_cutoff,
            alpha_mode: material.alpha_mode,
            double_sided: material.double_sided,
        })
    }

    pub fn from_gltf(data: Vec<u8>) -> anyhow::Result<Self> {
        let (gltf, buffers, images) = gltf::import_slice(data)?;

        let materials = gltf.materials().map(RawMaterial::from_gltf).collect::<Vec<_>>();
        let samplers = gltf.samplers().map(Sampler::from_gltf).collect::<Vec<_>>();

        let mut textures = Vec::new();
        let mut texture_headers = Vec::new();

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

        let mut node_headers = Vec::new();
        let mut primitive_headers = Vec::new();
        let mut uv_headers: Vec<TexCoordHeader> = Vec::new();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut uv_sets = Vec::new();

        for node in scene.nodes() {
            if let Some(mesh) = node.mesh() {
                let (position, rotation, scale) = node.transform().decomposed();
                node_headers.push(NodeHeader {
                    position,
                    rotation,
                    scale,
                    primitive_header_offset: std::mem::size_of::<PrimitiveHeader>() * primitive_headers.len(),
                    primitive_count: mesh.primitives().len(),
                });

                for primitive in mesh.primitives() {
                    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                    let mut primitive_uv_headers = Vec::new();

                    for set_index in 0..6 {
                        if let Some(uv_reader) = reader.read_tex_coords(set_index) {
                            let uv_set = uv_reader
                                .into_f32()
                                .map(|uv| TextureCoordinate::new(uv))
                                .collect::<Vec<_>>();
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

                    let primitive_indices: Vec<u32> = reader
                        .read_indices()
                        .map(|iter| iter.into_u32().collect())
                        .unwrap_or_default();

                    let positions: Vec<glam::Vec3> = reader
                        .read_positions()
                        .map(|iter| iter.map(glam::Vec3::from_array).collect())
                        .unwrap_or_default();

                    let normals = reader
                        .read_normals()
                        .map(|iter| iter.map(glam::Vec3::from_array).collect())
                        .unwrap_or_else(|| calculate_normals(&positions, &indices));

                    let uv_slice = &uv_sets[..primitive_uv_headers[0].count];
                    let tangents = reader
                        .read_tangents()
                        .map(|iter| iter.map(glam::Vec4::from_array).collect())
                        .unwrap_or_else(|| calculate_tangents(&positions, &normals, &primitive_indices, uv_slice));

                    let primitive_vertices = positions
                        .into_iter()
                        .zip(normals)
                        .zip(tangents)
                        .map(|((position, normal), tangent)| MeshVertex::new(position, normal, tangent))
                        .collect::<Vec<_>>();

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
            node_headers,
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
            let mut load_texture = async |obj_texture: &Option<String>| -> anyhow::Result<Option<usize>> {
                if let Some(filename) = obj_texture {
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

                    texture_headers.push(header);
                    textures.extend_from_slice(buffer);

                    Ok(Some(texture_headers.len() - 1))
                } else {
                    Ok(None)
                }
            };

            let diffuse_index = load_texture(&material.diffuse_texture).await?;
            let normal_index = load_texture(&material.normal_texture).await?;

            let new_material = RawMaterial::from_obj(&material);
            materials.push(new_material);

            // if let Some(filename) = &material.diffuse_texture {
            //     let texture_path = path.create_relative(&filename);
            //     let texture = texture_path.load_binary().await?;
            //     let image = image::load_from_memory(&texture)?.to_rgba8();
            //     let buffer = image.as_bytes();
            //     let header = TextureHeader {
            //         offset: textures.len(),
            //         size: buffer.len(),
            //         width: image.width(),
            //         height: image.height(),
            //         format: TextureFormat::RGBA8,
            //     };

            // texture_headers.push(header);
            // textures.extend(buffer);
        }

        let (node_headers, primitive_headers, uv_headers, vertices, indices, uv_sets) = models.into_iter().fold(
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |accumulator, model| {
                let (mut node_headers, mut primitive_headers, mut uv_headers, mut vertices, mut indices, mut uv_sets) =
                    accumulator;
                node_headers.push(NodeHeader {
                    position: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                    primitive_header_offset: std::mem::size_of::<PrimitiveHeader>() * primitive_headers.len(),
                    primitive_count: 1,
                });

                // let (model_vertices, tex_coords): (Vec<_>, Vec<_>) = (0..model.mesh.positions.len() / 3)
                //     .map(|i| {
                //         if model.mesh.normals.is_empty() {
                //             (
                //                 MeshVertex {
                //                     position: [
                //                         model.mesh.positions[i * 3],
                //                         model.mesh.positions[i * 3 + 1],
                //                         model.mesh.positions[i * 3 + 2],
                //                     ],
                //                     normal: [0.0, 0.0, 0.0],
                //                 },
                //                 TextureCoordinate([model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]]),
                //             )
                //         } else {
                //             (
                //                 MeshVertex {
                //                     position: [
                //                         model.mesh.positions[i * 3],
                //                         model.mesh.positions[i * 3 + 1],
                //                         model.mesh.positions[i * 3 + 2],
                //                     ],
                //                     normal: [
                //                         model.mesh.normals[i * 3],
                //                         model.mesh.normals[i * 3 + 1],
                //                         model.mesh.normals[i * 3 + 2],
                //                     ],
                //                 },
                //                 TextureCoordinate([model.mesh.texcoords[i * 2], 1.0 - model.mesh.texcoords[i * 2 + 1]]),
                //             )
                //         }
                //     })
                //     .unzip();

                let positions = model
                    .mesh
                    .positions
                    .chunks_exact(3)
                    .map(|vertices| glam::Vec3::from_slice(vertices))
                    .collect::<Vec<_>>();

                let tex_coords = model
                    .mesh
                    .texcoords
                    .chunks_exact(2)
                    .map(|uvs| TextureCoordinate::from_slice(uvs))
                    .collect::<Vec<_>>();

                let normals = if model.mesh.normals.is_empty() {
                    calculate_normals(&positions, &model.mesh.indices)
                } else {
                    model
                        .mesh
                        .normals
                        .chunks_exact(3)
                        .map(|normals| glam::Vec3::from_slice(normals))
                        .collect::<Vec<_>>()
                };

                let tangents = calculate_tangents(&positions, &normals, &model.mesh.indices, &tex_coords);

                let model_vertices = positions
                    .into_iter()
                    .zip(normals)
                    .zip(tangents)
                    .map(|((position, normal), tangent)| MeshVertex::new(position, normal, tangent))
                    .collect::<Vec<_>>();

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

                (node_headers, primitive_headers, uv_headers, vertices, indices, uv_sets)
            },
        );

        Ok(Self::new(
            node_headers,
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

pub fn unit_cube() -> (Vec<MeshVertex>, Vec<u32>, Vec<TextureCoordinate>) {
    use glam::{Vec2, Vec3};

    let positions = [
        // front face
        (Vec3::new(-0.5, -0.5, 0.5), Vec3::Z, Vec2::new(0.0, 0.0)),
        (Vec3::new(0.5, -0.5, 0.5), Vec3::Z, Vec2::new(1.0, 0.0)),
        (Vec3::new(0.5, 0.5, 0.5), Vec3::Z, Vec2::new(1.0, 1.0)),
        (Vec3::new(-0.5, 0.5, 0.5), Vec3::Z, Vec2::new(0.0, 1.0)),
        // back face
        (Vec3::new(0.5, -0.5, -0.5), -Vec3::Z, Vec2::new(0.0, 0.0)),
        (Vec3::new(-0.5, -0.5, -0.5), -Vec3::Z, Vec2::new(1.0, 0.0)),
        (Vec3::new(-0.5, 0.5, -0.5), -Vec3::Z, Vec2::new(1.0, 1.0)),
        (Vec3::new(0.5, 0.5, -0.5), -Vec3::Z, Vec2::new(0.0, 1.0)),
        // left face
        (Vec3::new(-0.5, -0.5, -0.5), -Vec3::X, Vec2::new(0.0, 0.0)),
        (Vec3::new(-0.5, -0.5, 0.5), -Vec3::X, Vec2::new(1.0, 0.0)),
        (Vec3::new(-0.5, 0.5, 0.5), -Vec3::X, Vec2::new(1.0, 1.0)),
        (Vec3::new(-0.5, 0.5, -0.5), -Vec3::X, Vec2::new(0.0, 1.0)),
        // right face
        (Vec3::new(0.5, -0.5, 0.5), Vec3::X, Vec2::new(0.0, 0.0)),
        (Vec3::new(0.5, -0.5, -0.5), Vec3::X, Vec2::new(1.0, 0.0)),
        (Vec3::new(0.5, 0.5, -0.5), Vec3::X, Vec2::new(1.0, 1.0)),
        (Vec3::new(0.5, 0.5, 0.5), Vec3::X, Vec2::new(0.0, 1.0)),
        // top face
        (Vec3::new(-0.5, 0.5, 0.5), Vec3::Y, Vec2::new(0.0, 0.0)),
        (Vec3::new(0.5, 0.5, 0.5), Vec3::Y, Vec2::new(1.0, 0.0)),
        (Vec3::new(0.5, 0.5, -0.5), Vec3::Y, Vec2::new(1.0, 1.0)),
        (Vec3::new(-0.5, 0.5, -0.5), Vec3::Y, Vec2::new(0.0, 1.0)),
        // bottom face
        (Vec3::new(-0.5, -0.5, -0.5), -Vec3::Y, Vec2::new(0.0, 0.0)),
        (Vec3::new(0.5, -0.5, -0.5), -Vec3::Y, Vec2::new(1.0, 0.0)),
        (Vec3::new(0.5, -0.5, 0.5), -Vec3::Y, Vec2::new(1.0, 1.0)),
        (Vec3::new(-0.5, -0.5, 0.5), -Vec3::Y, Vec2::new(0.0, 1.0)),
    ];

    // 12 triangles (2 per face)
    let indices: Vec<u32> = vec![
        0, 1, 2, 2, 3, 0, // front
        4, 5, 6, 6, 7, 4, // back
        8, 9, 10, 10, 11, 8, // left
        12, 13, 14, 14, 15, 12, // right
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    for (pos, normal, uv) in positions {
        vertices.push(pos);
        normals.push(normal);
        uvs.push(TextureCoordinate(uv.to_array()));
    }

    let tangents = calculate_tangents(&vertices, &normals, &indices, &uvs);

    let (vertices, uv_set): (Vec<MeshVertex>, Vec<TextureCoordinate>) = positions
        .into_iter()
        .zip(tangents)
        .map(|((pos, normal, uv), tangent)| (MeshVertex::new(pos, normal, tangent), TextureCoordinate(uv.to_array())))
        .collect();

    (vertices, indices, uv_set)
}
