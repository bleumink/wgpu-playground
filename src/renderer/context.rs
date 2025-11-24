use std::cell::OnceCell;

use crate::renderer::{hdr::HdrPipeline, texture::Texture};

pub struct RenderContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub environment_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub depth_texture: Texture,
    pub pending_resize: Option<wgpu::SurfaceConfiguration>,
    pub placeholder_texture: OnceCell<Texture>,
    pub hdr: HdrPipeline,
}

impl RenderContext {
    pub const MAX_UV_SETS: usize = 6;
    pub const TEXTURE_COUNT: usize = 5;

    pub async fn new(adapter: &wgpu::Adapter, config: wgpu::SurfaceConfiguration) -> anyhow::Result<Self> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: if cfg!(target_family = "wasm") {
                    wgpu::Limits::downlevel_defaults()
                } else {
                    wgpu::Limits { ..Default::default() }
                },
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let mut bind_group_layout_entries = Vec::new();
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });

        (0..Self::TEXTURE_COUNT).for_each(|index| {
            bind_group_layout_entries.extend_from_slice(&[
                wgpu::BindGroupLayoutEntry {
                    binding: (index * 2 + 1) as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: (index * 2 + 2) as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ]);
        });

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture bind group layout"),
            entries: &bind_group_layout_entries,
        });

        let environment_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Environment map bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let placeholder_texture = OnceCell::new();
        let depth_texture = Texture::create_depth_texture(&device, &config, Some("Depth texture"));
        let hdr = HdrPipeline::new(&device, &config);

        Ok(Self {
            device,
            queue,
            config,
            texture_bind_group_layout,
            environment_bind_group_layout,
            camera_bind_group_layout,
            depth_texture,
            pending_resize: None,
            placeholder_texture,
            hdr,
        })
    }

    pub fn placeholder_texture(&self) -> Texture {
        let texture = self
            .placeholder_texture
            .get_or_init(|| Texture::create_placeholder(&self.device, &self.queue));

        texture.clone()
    }

    pub fn resize(&mut self, config: wgpu::SurfaceConfiguration) {
        self.config = config;
        self.depth_texture = Texture::create_depth_texture(&self.device, &self.config, Some("Depth texture"));
        self.hdr.resize(&self.device, &self.config);
    }
}
