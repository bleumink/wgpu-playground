use std::{collections::{BTreeMap, HashMap}, num::NonZeroU32};

use bytemuck::{Pod, Zeroable};
use gltf::{
    image::Format as GltfImageFormat,
    texture::{MagFilter, MinFilter, WrappingMode},
};
use image::GenericImageView;
use rectangle_pack::{GroupedRectsToPlace, PackedLocation, RectToInsert};

use crate::renderer::{context::RenderContext, mesh::TextureCoordinate};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct TextureFormat(pub usize);

impl TextureFormat {
    pub const RGBA8: Self = Self(0);
    pub const RGB8: Self = Self(1);
    pub const RG8: Self = Self(2);
    pub const R8: Self = Self(3);

    fn make_image<F, P>(width: u32, height: u32, data: &[u8], func: F) -> Option<image::DynamicImage>
    where
        F: FnOnce(image::ImageBuffer<P, Vec<u8>>) -> image::DynamicImage,
        P: image::Pixel<Subpixel = u8>,
    {
        image::ImageBuffer::from_raw(width, height, data.to_vec()).map(func)
    }

    pub fn from_gltf(format: &GltfImageFormat) -> Self {
        match format {
            GltfImageFormat::R8G8B8A8 => Self::RGBA8,
            GltfImageFormat::R8G8B8 => Self::RGB8,
            GltfImageFormat::R8G8 => Self::RG8,
            GltfImageFormat::R8 => Self::R8,
            _ => panic!("Unsupported texture format"),
        }
    }

    pub fn to_image(self, width: u32, height: u32, data: &[u8]) -> Option<image::DynamicImage> {
        match self {
            Self::RGBA8 => Self::make_image(width, height, data, image::DynamicImage::ImageRgba8),
            Self::RGB8 => Self::make_image(width, height, data, image::DynamicImage::ImageRgb8),
            Self::RG8 => Self::make_image(width, height, data, image::DynamicImage::ImageLumaA8),
            Self::R8 => Self::make_image(width, height, data, image::DynamicImage::ImageLuma8),
            _ => panic!("Unsupported texture format"),
        }
    }
}

#[derive(Debug)]
pub struct TextureView<'a> {
    pub texture: &'a [u8],
    pub sampler: Sampler,
    pub uv_index: u32,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub is_srgb: bool,
}

impl TextureView<'_> {
    pub fn to_image(&self) -> Option<image::DynamicImage> {
        self.format.to_image(self.width, self.height, self.texture)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Pod, Zeroable)]
pub struct Sampler {
    pub mag_filter: u8,
    pub min_filter: u8,
    pub mipmap_filter: u8,
    pub address_mode_u: u8,
    pub address_mode_v: u8,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            mag_filter: 1,
            min_filter: 1,
            mipmap_filter: 1,
            address_mode_u: 2,
            address_mode_v: 2,
        }
    }
}

impl Sampler {
    fn to_filter_mode(value: u8) -> wgpu::FilterMode {
        match value {
            0 => wgpu::FilterMode::Nearest,
            1 => wgpu::FilterMode::Linear,
            _ => panic!("Invalid filter mode"),
        }
    }

    fn to_address_mode(value: u8) -> wgpu::AddressMode {
        match value {
            0 => wgpu::AddressMode::ClampToEdge,
            1 => wgpu::AddressMode::MirrorRepeat,
            2 => wgpu::AddressMode::Repeat,
            _ => panic!("Invalid address mode"),
        }
    }

    fn get_filters(&self) -> (wgpu::FilterMode, wgpu::FilterMode, wgpu::FilterMode) {
        (
            Self::to_filter_mode(self.mag_filter),
            Self::to_filter_mode(self.min_filter),
            Self::to_filter_mode(self.mipmap_filter),
        )
    }

    pub fn from_gltf(sampler: gltf::texture::Sampler) -> Self {
        let (min_filter, mipmap_filter) = match sampler.min_filter() {
            Some(MinFilter::Nearest) => (0, 0),
            Some(MinFilter::Linear) => (1, 0),
            Some(MinFilter::NearestMipmapNearest) => (0, 0),
            Some(MinFilter::LinearMipmapNearest) => (1, 0),
            Some(MinFilter::NearestMipmapLinear) => (0, 1),
            Some(MinFilter::LinearMipmapLinear) => (1, 1),
            None => (1, 1),
        };

        let mag_filter = match sampler.mag_filter().unwrap_or(MagFilter::Linear) {
            MagFilter::Nearest => 0,
            MagFilter::Linear => 1,
        };

        let address_mode_u = match sampler.wrap_s() {
            WrappingMode::ClampToEdge => 0,
            WrappingMode::MirroredRepeat => 1,
            WrappingMode::Repeat => 2,
        };

        let address_mode_v = match sampler.wrap_s() {
            WrappingMode::ClampToEdge => 0,
            WrappingMode::MirroredRepeat => 1,
            WrappingMode::Repeat => 2,
        };

        Sampler {
            mag_filter,
            min_filter,
            mipmap_filter,
            address_mode_u,
            address_mode_v,
        }
    }

    pub fn to_wgpu_descriptor(&self) -> wgpu::SamplerDescriptor<'_> {
        let (mag_filter, min_filter, mipmap_filter) = self.get_filters();

        wgpu::SamplerDescriptor {
            address_mode_u: Self::to_address_mode(self.address_mode_u),
            address_mode_v: Self::to_address_mode(self.address_mode_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter,
            min_filter,
            mipmap_filter,
            ..Default::default()
        }
    }
}

// pub struct TextureData {
//     width: u32,
//     height: u32,
//     format: TextureFormat,
//     data: Vec<u8>,
//     is_srgb: bool,
// }

// impl From<TextureView<'_>> for TextureData {
//     fn from(value: TextureView) -> Self {
//         Self {
//             width: value.width,
//             height: value.height,
//             format: value.format,
//             data: value.texture.to_vec(),
//             is_srgb: value.is_srgb,
//         }
//     }
// }

// #[derive(Copy, Clone, Debug)]
// pub struct TextureHandle {
//     pub texture_index: u32,
//     pub sampler_index: u32,
//     pub uv_index: u32,
// }

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureHandle {
    pub atlas_index: u32,
    pub uv_index: u32,
    pub sampler_index: u32,
    pub uv_min: TextureCoordinate,
    pub uv_max: TextureCoordinate,
}

impl Default for TextureHandle {
    fn default() -> Self {
        Self {
            atlas_index: 0,
            uv_index: 0,
            sampler_index: 0,
            uv_min: TextureCoordinate::default(),
            uv_max: TextureCoordinate::default(),
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
pub struct TextureReference {
    pub texture_index: u32,
    pub uv_index: u32,
    pub sampler_index: u32,
}

unsafe impl bytemuck::ZeroableInOption for TextureReference {}
unsafe impl bytemuck::PodInOption for TextureReference {}

impl Default for TextureReference {
    fn default() -> Self {
        Self {
            texture_index: 0,
            uv_index: 0,
            sampler_index: 0,
        }
    }
}

impl TextureReference {
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

pub struct SamplerPool {
    samplers: Vec<wgpu::Sampler>,
    cache: HashMap<Sampler, u32>,
}

impl SamplerPool {
    pub fn new() -> Self {
        Self {
            samplers: Vec::new(),
            cache: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, sampler: Sampler, context: &RenderContext) -> u32 {
        if let Some(&sampler) = self.cache.get(&sampler) {
            return sampler;
        }

        let wgpu_sampler = context.device.create_sampler(&sampler.to_wgpu_descriptor());
        let index = self.samplers.len() as u32;
        
        self.samplers.push(wgpu_sampler);
        self.cache.insert(sampler, index);

        index
    }

    pub fn get_by_index(&self, index: usize) -> Option<&wgpu::Sampler> {
        self.samplers.get(index)
    }

    pub fn samplers(&self) -> &[wgpu::Sampler] {
        &self.samplers
    }
}

struct PackedRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    uv_min: TextureCoordinate,
    uv_max: TextureCoordinate,
}

pub struct TextureAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,    
    source: image::RgbaImage,
    atlas_size: u32,
    padding_size: u32,

    packer: GroupedRectsToPlace<u32>,
    packed_locations: Vec<PackedRegion>,

    samplers: SamplerPool,
    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,    
    is_dirty: bool,
}

impl TextureAtlas {
    pub fn new(atlas_size: u32, padding_size: u32, context: &RenderContext) -> Self {
        let source = image::RgbaImage::new(atlas_size, atlas_size);
        let texture = context.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,                
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[]
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let layout = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture atlas layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { 
                        sample_type: wgpu::TextureSampleType::Float { filterable: true }, 
                        view_dimension: wgpu::TextureViewDimension::D2, 
                        multisampled: false
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ]
        });

        let mut samplers = SamplerPool::new();
        let sampler_index = samplers.get_or_create(Sampler::default(), context);
        let sampler = samplers.get_by_index(sampler_index as usize).unwrap();

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture atlas bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                }
            ]
        });

        Self {
            texture,
            view,            
            source,
            atlas_size,
            padding_size,            
            packer: GroupedRectsToPlace::new(),
            packed_locations: Vec::new(),            
            samplers: SamplerPool::new(),
            bind_group,
            layout,
            is_dirty: false,
        }
    }

    pub fn add_texture(&mut self, view: TextureView, context: &RenderContext) -> Option<TextureHandle> {
        let sampler_index = self.samplers.get_or_create(view.sampler, context);

        let image = view.to_image()?;
        let (width, height) = image.dimensions();
        let padded_width = width + self.padding_size * 2;        
        let padded_height = height + self.padding_size * 2;

        let texture_id = self.packed_locations.len() as u32;
        self.packer.push_rect(
            texture_id, 
            None, 
            RectToInsert::new(padded_width, padded_height, 1)
        );

        let mut target_bins = BTreeMap::new();
        target_bins.insert(0, rectangle_pack::TargetBin::new(self.atlas_size, self.atlas_size, 1));

        let pack_result = rectangle_pack::pack_rects(
            &self.packer, 
            &mut target_bins, 
            &rectangle_pack::volume_heuristic, 
            &rectangle_pack::contains_smallest_box
        ).ok()?;

        let (_, packed_location) = pack_result.packed_locations().get(&texture_id)?;
        let (x, y) = (packed_location.x(), packed_location.y());
        
        copy_with_edge_extrusion(&mut self.source, &image, x, y, self.padding_size);
        self.write_texture(x, y, width, height, context);

        let unpadded_x = x + self.padding_size;
        let unpadded_y = y + self.padding_size;
       
        let uv_min = TextureCoordinate::new([
            unpadded_x as f32 / self.atlas_size as f32,
            unpadded_y as f32 / self.atlas_size as f32,
        ]);
        let uv_max = TextureCoordinate::new([
            (unpadded_x + width) as f32 / self.atlas_size as f32,
            (unpadded_y + height) as f32 / self.atlas_size as f32,
        ]);

        self.packed_locations.push(PackedRegion { 
            x, 
            y, 
            width: padded_width, 
            height: padded_height, 
            uv_min, 
            uv_max 
        });

        let handle = TextureHandle {
            atlas_index: 0,
            uv_index: view.uv_index,
            sampler_index,
            uv_min,
            uv_max
        };

        self.is_dirty = true;
        Some(handle)
    }

    pub fn sync(&mut self, context: &RenderContext) {        
        let sampler_refs = self.samplers.samplers().iter().collect::<Vec<_>>();
        self.bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture atlas bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(&sampler_refs),
                }
            ]
        });
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }    

    pub fn samplers(&self) -> &SamplerPool {
        &self.samplers
    }

    pub fn samplers_mut(&mut self) -> &mut SamplerPool {
        &mut self.samplers
    }

    pub fn is_dirty(&mut self) -> bool {
        let dirty = self.is_dirty;
        self.is_dirty = false;
        dirty
    }

    fn write_texture(&self, x: u32, y: u32, width: u32, height: u32, context: &RenderContext) {
        context.queue.write_texture(wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,            
            },
            bytemuck::cast_slice(
                &self.source.view(x, y, width, height).to_image()
            ),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),                       
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            }
        );        
    }
}
// #[derive(Clone, Debug)]
// pub struct TextureInstance {
//     pub texture: Texture,
//     pub uv_index: u32,
// }

#[derive(Clone, Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn create_placeholder(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let data = [255u8, 255, 255, 255];
        let size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        Self::from_bytes(
            device,
            queue,
            &data,
            size,
            wgpu::TextureFormat::Rgba8Unorm,
            &Sampler::default().to_wgpu_descriptor(),
            Some("placeholder"),
        )
    }

    pub fn from_view(device: &wgpu::Device, queue: &wgpu::Queue, view: &TextureView, label: Option<&str>) -> Self {
        let image = view.to_image().unwrap();
        let format = if view.is_srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let data = image.to_rgba8();
        let dimensions = image.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        Self::from_bytes(device, queue, &data, size, format, &view.sampler.to_wgpu_descriptor(), label)
    }

    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        sampler_desc: &wgpu::SamplerDescriptor,
        label: Option<&str>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );

        let sampler = device.create_sampler(sampler_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view, sampler }
    }

    // pub fn from_image(
    //     device: &wgpu::Device,
    //     queue: &wgpu::Queue,
    //     img: &image::DynamicImage,
    //     label: Option<&str>,
    // ) -> Self {
    //     let rgba = img.to_rgba8();
    //     let dimensions = img.dimensions();
    //     let size = wgpu::Extent3d {
    //         width: dimensions.0,
    //         height: dimensions.1,
    //         depth_or_array_layers: 1,
    //     };

    //     let texture = device.create_texture(&wgpu::TextureDescriptor {
    //         label,
    //         size,
    //         mip_level_count: 1,
    //         sample_count: 1,
    //         dimension: wgpu::TextureDimension::D2,
    //         format: wgpu::TextureFormat::Rgba8UnormSrgb,
    //         usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    //         view_formats: &[],
    //     });

    //     queue.write_texture(
    //         wgpu::TexelCopyTextureInfo {
    //             texture: &texture,
    //             mip_level: 0,
    //             origin: wgpu::Origin3d::ZERO,
    //             aspect: wgpu::TextureAspect::All,
    //         },
    //         &rgba,
    //         wgpu::TexelCopyBufferLayout {
    //             offset: 0,
    //             bytes_per_row: Some(4 * dimensions.0),
    //             rows_per_image: Some(dimensions.1),
    //         },
    //         size,
    //     );

    //     let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    //     // let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    //     //     address_mode_u: wgpu::AddressMode::Repeat,
    //     //     address_mode_v: wgpu::AddressMode::Repeat,
    //     //     address_mode_w: wgpu::AddressMode::Repeat,
    //     //     mag_filter: wgpu::FilterMode::Linear,
    //     //     min_filter: wgpu::FilterMode::Linear,
    //     //     mipmap_filter: wgpu::FilterMode::Linear,
    //     //     ..Default::default()
    //     // });
    //     let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    //         address_mode_u: wgpu::AddressMode::ClampToEdge,
    //         address_mode_v: wgpu::AddressMode::ClampToEdge,
    //         address_mode_w: wgpu::AddressMode::ClampToEdge,
    //         mag_filter: wgpu::FilterMode::Linear,
    //         min_filter: wgpu::FilterMode::Nearest,
    //         mipmap_filter: wgpu::FilterMode::Nearest,
    //         ..Default::default()
    //     });

    //     Self { texture, view, sampler }
    // }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        label: Option<&str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self { texture, view, sampler }
    }

    pub fn create_2d_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sampler_descriptor: &wgpu::SamplerDescriptor,
        label: Option<&str>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&sampler_descriptor);

        Self { texture, view, sampler }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }
}

pub trait CubemapData {
    const FORMAT: wgpu::TextureFormat;
}

impl CubemapData for f32 {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
}

impl CubemapData for half::f16 {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
}

#[derive(Clone, Debug)]
pub struct CubeTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl CubeTexture {
    pub fn create_2d_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sampler: wgpu::Sampler,
        label: Option<&str>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label,
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        Self { texture, view, sampler }
    }

    pub fn create_placeholder<T: Pod + CubemapData>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        value: &[T; 4],
        filter_mode: wgpu::FilterMode,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder cubemap"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: T::FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for layer in 0..6 {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: layer },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(value),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: T::FORMAT.target_pixel_byte_cost(),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Placeholder cubemap view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Placeholder sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self { texture, view, sampler }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.texture.format()
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }
}

fn copy_with_edge_extrusion(
    atlas_image: &mut image::RgbaImage,
    source: &image::DynamicImage,
    x: u32,
    y: u32,
    padding: u32,
) {
    let rgba_source = source.to_rgba8();
    let (width, height) = source.dimensions();

    // Copy main texture
    for py in 0..height {
        for px in 0..width {
            let pixel = rgba_source.get_pixel(px, py);
            atlas_image.put_pixel(x + padding + px, y + padding + py, *pixel);
        }
    }

    // Extrude top edge
    for px in 0..width {
        let edge_pixel = rgba_source.get_pixel(px, 0);
        for pad in 0..padding {
            atlas_image.put_pixel(x + padding + px, y + pad, *edge_pixel);
        }
    }

    // Extrude bottom edge
    for px in 0..width {
        let edge_pixel = rgba_source.get_pixel(px, height - 1);
        for pad in 1..=padding {
            atlas_image.put_pixel(x + padding + px, y + padding + height - 1 + pad, *edge_pixel);
        }
    }

    // Extrude left edge
    for py in 0..height {
        let edge_pixel = rgba_source.get_pixel(0, py);
        for pad in 0..padding {
            atlas_image.put_pixel(x + pad, y + padding + py, *edge_pixel);
        }
    }

    // Extrude right edge
    for py in 0..height {
        let edge_pixel = rgba_source.get_pixel(width - 1, py);
        for pad in 1..=padding {
            atlas_image.put_pixel(x + padding + width - 1 + pad, y + padding + py, *edge_pixel);
        }
    }

    // Extrude corners
    let top_left = rgba_source.get_pixel(0, 0);
    let top_right = rgba_source.get_pixel(width - 1, 0);
    let bottom_left = rgba_source.get_pixel(0, height - 1);
    let bottom_right = rgba_source.get_pixel(width - 1, height - 1);

    for py in 0..padding {
        for px in 0..padding {
            atlas_image.put_pixel(x + px, y + py, *top_left);
            atlas_image.put_pixel(x + padding + width + px, y + py, *top_right);
            atlas_image.put_pixel(x + px, y + padding + height + py, *bottom_left);
            atlas_image.put_pixel(x + padding + width + px, y + padding + height + py, *bottom_right);
        }
    }
}