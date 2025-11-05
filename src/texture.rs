use bytemuck::{Pod, Zeroable};
use gltf::{
    image::Format as GltfImageFormat,
    texture::{MagFilter, MinFilter, WrappingMode},
};
use image::GenericImageView;

use crate::{context::RenderContext, material::MaterialView};

#[repr(transparent)]
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
}

impl TextureView<'_> {
    pub fn to_image(&self) -> Option<image::DynamicImage> {
        self.format.to_image(self.width, self.height, self.texture)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
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

    pub fn desc(&self) -> wgpu::SamplerDescriptor<'_> {
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

#[derive(Clone, Debug)]
pub struct TextureInstance {
    pub texture: Texture,
    pub uv_index: u32,
}

#[derive(Clone, Debug)]
pub struct Texture {
    #[allow(unused)]
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
            &Sampler::default().desc(),
            Some("placeholder"),
        )
    }

    pub fn from_view(device: &wgpu::Device, queue: &wgpu::Queue, view: &TextureView, label: Option<&str>) -> Self {
        let image = view.to_image().unwrap();
        let data = image.to_rgba8();
        let dimensions = image.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        Self::from_bytes(device, queue, &data, size, &view.sampler.desc(), label)
    }

    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        size: wgpu::Extent3d,
        sampler_desc: &wgpu::SamplerDescriptor,
        label: Option<&str>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &image::DynamicImage,
        label: Option<&str>,
    ) -> Self {
        let rgba = img.to_rgba8();
        let dimensions = img.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        //     address_mode_u: wgpu::AddressMode::Repeat,
        //     address_mode_v: wgpu::AddressMode::Repeat,
        //     address_mode_w: wgpu::AddressMode::Repeat,
        //     mag_filter: wgpu::FilterMode::Linear,
        //     min_filter: wgpu::FilterMode::Linear,
        //     mipmap_filter: wgpu::FilterMode::Linear,
        //     ..Default::default()
        // });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self { texture, view, sampler }
    }

    pub fn create_depth_texture(
        label: Option<&str>,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
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
}
