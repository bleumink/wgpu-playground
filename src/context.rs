use std::{cell::OnceCell, rc::Rc, sync::Arc};

use winit::window::Window;

use crate::{surface::Surface, texture::Texture};

pub struct RenderContext {
    pub window: Arc<Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub depth_texture: Texture,
    pub pending_resize: Option<wgpu::SurfaceConfiguration>,
    pub placeholder_texture: OnceCell<Texture>,
}

impl RenderContext {
    pub const MAX_UV_SETS: usize = 6;
    pub const TEXTURE_COUNT: usize = 5;

    pub async fn new(
        window: Arc<Window>,
        adapter: &wgpu::Adapter,
        config: wgpu::SurfaceConfiguration,
    ) -> anyhow::Result<Self> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: if cfg!(target_family = "wasm") {
                    wgpu::Limits::downlevel_defaults()
                } else {
                    wgpu::Limits { ..Default::default() }
                },
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let bind_group_layout_entries = (0..Self::TEXTURE_COUNT)
            .flat_map(|index| {
                [
                    wgpu::BindGroupLayoutEntry {
                        binding: (index * 2) as u32,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: (index * 2 + 1) as u32,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },                    
                ]
            })
            .collect::<Vec<_>>();

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture bind group layout"),
            entries: &bind_group_layout_entries,
        });

        let placeholder_texture = OnceCell::new();
        let depth_texture = Texture::create_depth_texture(Some("Depth texture"), &device, &config);

        Ok(Self {
            window,
            device,
            queue,
            config,
            texture_bind_group_layout,
            depth_texture,
            pending_resize: None,
            placeholder_texture,
        })
    }

    pub fn placeholder_texture(&self) -> Texture {
        let texture = self.placeholder_texture.get_or_init(
            || Texture::create_placeholder(&self.device, &self.queue)
        );

        texture.clone()
    }

    pub fn resize(&mut self, config: wgpu::SurfaceConfiguration) {
        self.config = config;
        self.depth_texture = Texture::create_depth_texture(Some("Depth texture"), &self.device, &self.config);
    }
}
