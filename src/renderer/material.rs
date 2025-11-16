use bytemuck::{Pod, Zeroable};
use gltf::material::AlphaMode;
use wgpu::util::DeviceExt;

use crate::renderer::{
    context::RenderContext,
    texture::{Texture, TextureInstance, TextureView},
};

pub enum TextureInstanceSlot {
    BaseColor,
    MetallicRoughness,
    Normal,
    Occlusion,
    Emissive,
}

impl TextureInstanceSlot {
    pub const COUNT: u32 = 5;
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 3],
    _padding0: u32,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub occlusion_strength: f32,
    pub normal_scale: f32,
    pub alpha_cutoff: f32,
    pub alpha_mode: u32,
    pub double_sided: u32,
    _padding1: u32,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub uniform: MaterialUniform,
    pub uniform_buffer: wgpu::Buffer,
    pub textures: Vec<TextureInstance>,
    pub bind_group: wgpu::BindGroup,
}

impl Material {
    pub fn new(material: MaterialView, label: Option<&str>, context: &RenderContext) -> Self {
        let material_textures = [
            material.base_color,
            material.metallic_roughness,
            material.normal,
            material.occlusion,
            material.emissive,
        ];

        let textures = material_textures
            .iter()
            .enumerate()
            .map(|(index, maybe_view)| {
                if let Some(view) = maybe_view {
                    TextureInstance {
                        texture: Texture::from_view(&context.device, &context.queue, view, label),
                        uv_index: view.uv_index,
                    }
                } else {
                    TextureInstance {
                        texture: context.placeholder_texture(),
                        uv_index: index as u32,
                    }
                }
            })
            .collect::<Vec<_>>();

        let uniform = MaterialUniform {
            base_color_factor: material.base_color_factor,
            emissive_factor: material.emissive_factor,
            metallic_factor: material.metallic_factor,
            roughness_factor: material.roughness_factor,
            occlusion_strength: material.occlusion_strength,
            normal_scale: material.normal_scale,
            alpha_cutoff: material.alpha_cutoff,
            alpha_mode: material.alpha_mode as u32,
            double_sided: material.double_sided as u32,
            _padding0: 0,
            _padding1: 0,
        };

        let uniform_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label,
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let mut bind_group_entries = Vec::new();
        bind_group_entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        });

        textures.iter().enumerate().for_each(|(index, texture_instance)| {
            bind_group_entries.extend_from_slice(&[
                wgpu::BindGroupEntry {
                    binding: (index * 2 + 1) as u32,
                    resource: wgpu::BindingResource::TextureView(&texture_instance.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: (index * 2 + 2) as u32,
                    resource: wgpu::BindingResource::Sampler(&texture_instance.texture.sampler),
                },
            ]);
        });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &context.texture_bind_group_layout,
            entries: &bind_group_entries,
        });

        Self {
            uniform,
            uniform_buffer,
            textures,
            bind_group,
        }
    }
}

pub struct MaterialView<'a> {
    pub base_color: Option<TextureView<'a>>,
    pub metallic_roughness: Option<TextureView<'a>>,
    pub normal: Option<TextureView<'a>>,
    pub occlusion: Option<TextureView<'a>>,
    pub emissive: Option<TextureView<'a>>,
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 3],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub occlusion_strength: f32,
    pub normal_scale: f32,
    pub alpha_cutoff: f32,
    pub alpha_mode: u8,
    pub double_sided: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RawMaterial {
    pub base_color: Option<TextureSlot>,
    pub metallic_roughness: Option<TextureSlot>,
    pub normal: Option<TextureSlot>,
    pub occlusion: Option<TextureSlot>,
    pub emissive: Option<TextureSlot>,
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 3],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub occlusion_strength: f32,
    pub normal_scale: f32,
    pub alpha_cutoff: f32,
    pub alpha_mode: u8,
    pub double_sided: u8,
    pub _padding: [u8; 2],
}

impl RawMaterial {
    pub fn from_gltf(material: gltf::Material) -> Self {
        let pbr = material.pbr_metallic_roughness();

        Self {
            base_color: TextureSlot::from_gltf(pbr.base_color_texture()),
            metallic_roughness: TextureSlot::from_gltf(pbr.metallic_roughness_texture()),
            normal: TextureSlot::from_gltf(material.normal_texture()),
            occlusion: TextureSlot::from_gltf(material.occlusion_texture()),
            emissive: TextureSlot::from_gltf(material.emissive_texture()),
            base_color_factor: pbr.base_color_factor(),
            emissive_factor: material.emissive_factor(),
            metallic_factor: pbr.metallic_factor(),
            roughness_factor: pbr.roughness_factor(),
            occlusion_strength: material.occlusion_texture().map(|t| t.strength()).unwrap_or(1.0),
            normal_scale: material.normal_texture().map(|t| t.scale()).unwrap_or(1.0),
            alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
            alpha_mode: match material.alpha_mode() {
                AlphaMode::Opaque => 0,
                AlphaMode::Mask => 1,
                AlphaMode::Blend => 2,
            },
            double_sided: material.double_sided() as u8,
            _padding: [0; 2],
        }
    }

    pub fn from_obj(material: &tobj::Material) -> Self {
        Self {
            base_color: Some(TextureSlot::default()),
            metallic_roughness: None,
            normal: Some(TextureSlot {
                texture_index: 1,
                uv_index: 0,
                sampler_index: 0,
            }),
            occlusion: None,
            emissive: None,
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            emissive_factor: [0.0, 0.0, 0.0],
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            occlusion_strength: 1.0,
            normal_scale: 1.0,
            alpha_cutoff: 0.5,
            alpha_mode: 0,
            double_sided: 0,
            _padding: [0; 2],
        }
    }
}

pub trait GltfTextureInfo {
    fn texture(&self) -> gltf::Texture<'_>;
    fn tex_coord(&self) -> u32;
}

impl GltfTextureInfo for gltf::texture::Info<'_> {
    fn texture(&self) -> gltf::Texture<'_> {
        self.texture()
    }
    fn tex_coord(&self) -> u32 {
        self.tex_coord()
    }
}

impl GltfTextureInfo for gltf::material::NormalTexture<'_> {
    fn texture(&self) -> gltf::Texture<'_> {
        self.texture()
    }
    fn tex_coord(&self) -> u32 {
        self.tex_coord()
    }
}

impl GltfTextureInfo for gltf::material::OcclusionTexture<'_> {
    fn texture(&self) -> gltf::Texture<'_> {
        self.texture()
    }
    fn tex_coord(&self) -> u32 {
        self.tex_coord()
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureSlot {
    pub texture_index: u32,
    pub uv_index: u32,
    pub sampler_index: u32,
}

unsafe impl bytemuck::ZeroableInOption for TextureSlot {}
unsafe impl bytemuck::PodInOption for TextureSlot {}

impl Default for TextureSlot {
    fn default() -> Self {
        Self {
            texture_index: 0,
            uv_index: 0,
            sampler_index: 0,
        }
    }
}

impl TextureSlot {
    pub fn from_gltf<T: GltfTextureInfo>(texture_info: Option<T>) -> Option<Self> {
        texture_info.and_then(|texture_info| {
            let slot = Self {
                texture_index: texture_info.texture().source().index() as u32,
                uv_index: texture_info.tex_coord() as u32,
                sampler_index: texture_info.texture().sampler().index().unwrap_or(0) as u32,
            };
            Some(slot)
        })
    }
}
