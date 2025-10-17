use bytemuck::{Pod, Zeroable};
use gltf::material::AlphaMode;

use crate::texture::{Texture, TextureView};

pub struct MaterialHandle {
    pub diffuse_texture: Texture,
    pub bind_group: wgpu::BindGroup,
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
pub struct Material {
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

impl Material {
    pub fn from_gltf(material: &gltf::Material) -> Self {
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
            normal: None,
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
    fn texture(&self) -> gltf::Texture<'_> { self.texture() }
    fn tex_coord(&self) -> u32 { self.tex_coord() }
}

impl GltfTextureInfo for gltf::material::NormalTexture<'_> {
    fn texture(&self) -> gltf::Texture<'_> { self.texture() }
    fn tex_coord(&self) -> u32 { self.tex_coord() }
}

impl GltfTextureInfo for gltf::material::OcclusionTexture<'_> {
    fn texture(&self) -> gltf::Texture<'_> { self.texture() }
    fn tex_coord(&self) -> u32 { self.tex_coord() }
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
        if let Some(texture_info) = texture_info {
            let slot = Self {
                texture_index: texture_info.texture().index() as u32,
                uv_index: texture_info.tex_coord() as u32,
                sampler_index: texture_info.texture().sampler().index().unwrap_or(0) as u32,
            };
            Some(slot)
        } else {
            None
            // Self::default()
        }
    }
}