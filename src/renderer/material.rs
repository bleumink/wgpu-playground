use bytemuck::{Pod, Zeroable};
use gltf::material::AlphaMode;
use uuid::Uuid;

use crate::renderer::{
    context::RenderContext,
    texture::{Texture, TextureAtlas, TextureHandle, TextureReference, TextureView},
};

// pub enum TextureInstanceSlot {
//     BaseColor,
//     MetallicRoughness,
//     Normal,
//     Occlusion,
//     Emissive,
// }

// impl TextureInstanceSlot {
//     pub const COUNT: u32 = 5;
// }

pub type MaterialId = Uuid;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub base_color: TextureHandle,
    pub metallic_roughness: TextureHandle,
    pub normal: TextureHandle,
    pub occlusion: TextureHandle,
    pub emissive: TextureHandle,
    _padding0: u32,
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 3],    
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub occlusion_strength: f32,
    pub normal_scale: f32,
    pub alpha_cutoff: f32,
    pub alpha_mode: u32,
    pub double_sided: u32,
    _padding1: [u32; 2],
}

impl MaterialUniform {
    pub fn from_view(view: MaterialView, textures: &mut TextureAtlas, context: &RenderContext) -> Self {          
        let mut create_texture = |view: Option<TextureView>| {
            view.and_then(|texture_view| textures.add_texture(texture_view, context)).unwrap_or_default()
        };

        Self {
            base_color: create_texture(view.base_color),
            metallic_roughness: create_texture(view.metallic_roughness),
            normal: create_texture(view.normal),
            occlusion: create_texture(view.occlusion),
            emissive: create_texture(view.emissive),            
            base_color_factor: view.base_color_factor,
            emissive_factor: view.emissive_factor,
            metallic_factor: view.metallic_factor,
            roughness_factor: view.roughness_factor,
            occlusion_strength: view.occlusion_strength,
            normal_scale: view.normal_scale,
            alpha_cutoff: view.alpha_cutoff,
            alpha_mode: view.alpha_mode as u32,
            double_sided: view.double_sided as u32,
            _padding0: 0,
            _padding1: [0, 0],
        }
    }
}


// #[derive(Clone, Debug)]
// pub struct Material {
//     pub uniform: MaterialUniform,
//     pub uniform_buffer: wgpu::Buffer,
//     pub textures: Vec<TextureInstance>,
//     pub bind_group: wgpu::BindGroup,
// }

// impl Material {
//     pub fn from_view(material: MaterialView, label: Option<&str>, context: &RenderContext) -> Self {
//         let material_textures = [
//             material.base_color,
//             material.metallic_roughness,
//             material.normal,
//             material.occlusion,
//             material.emissive,
//         ];

//         let textures = material_textures
//             .iter()
//             .enumerate()
//             .map(|(index, maybe_view)| {
//                 if let Some(view) = maybe_view {
//                     TextureInstance {
//                         texture: Texture::from_view(&context.device, &context.queue, view, label),
//                         uv_index: view.uv_index,
//                     }
//                 } else {
//                     TextureInstance {
//                         texture: context.placeholder_texture(),
//                         uv_index: index as u32,
//                     }
//                 }
//             })
//             .collect::<Vec<_>>();

//         let uniform = MaterialUniform {
//             base_color_factor: material.base_color_factor,
//             emissive_factor: material.emissive_factor,
//             metallic_factor: material.metallic_factor,
//             roughness_factor: material.roughness_factor,
//             occlusion_strength: material.occlusion_strength,
//             normal_scale: material.normal_scale,
//             alpha_cutoff: material.alpha_cutoff,
//             alpha_mode: material.alpha_mode as u32,
//             double_sided: material.double_sided as u32,
//             _padding0: 0,
//             _padding1: 0,
//         };

//         let uniform_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//             label,
//             contents: bytemuck::bytes_of(&uniform),
//             usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
//         });

//         let mut bind_group_entries = Vec::new();
//         bind_group_entries.push(wgpu::BindGroupEntry {
//             binding: 0,
//             resource: uniform_buffer.as_entire_binding(),
//         });

//         textures.iter().enumerate().for_each(|(index, texture_instance)| {
//             bind_group_entries.extend_from_slice(&[
//                 wgpu::BindGroupEntry {
//                     binding: (index * 2 + 1) as u32,
//                     resource: wgpu::BindingResource::TextureView(&texture_instance.texture.view),
//                 },
//                 wgpu::BindGroupEntry {
//                     binding: (index * 2 + 2) as u32,
//                     resource: wgpu::BindingResource::Sampler(&texture_instance.texture.sampler),
//                 },
//             ]);
//         });

//         let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
//             label,
//             layout: &context.texture_bind_group_layout,
//             entries: &bind_group_entries,
//         });

//         Self {
//             uniform,
//             uniform_buffer,
//             textures,
//             bind_group,
//         }
//     }
// }

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

// impl MaterialView<'_> {
//     pub fn to_owned(self, label: Option<&str>, context: &RenderContext) -> Material {
//         Material::from_view(self, label, context)
//     }
// }

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Material {
    pub base_color: Option<TextureReference>,
    pub metallic_roughness: Option<TextureReference>,
    pub normal: Option<TextureReference>,
    pub occlusion: Option<TextureReference>,
    pub emissive: Option<TextureReference>,
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
    pub fn from_gltf(material: gltf::Material) -> Self {
        let pbr = material.pbr_metallic_roughness();

        Self {
            base_color: TextureReference::from_gltf(pbr.base_color_texture()),
            metallic_roughness: TextureReference::from_gltf(pbr.metallic_roughness_texture()),
            normal: TextureReference::from_gltf(material.normal_texture()),
            occlusion: TextureReference::from_gltf(material.occlusion_texture()),
            emissive: TextureReference::from_gltf(material.emissive_texture()),
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
            base_color: Some(TextureReference::default()),
            metallic_roughness: None,
            normal: Some(TextureReference {
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


