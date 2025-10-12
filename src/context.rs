use std::sync::Arc;

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
}

impl RenderContext {
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

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let depth_texture = Texture::create_depth_texture(Some("Depth texture"), &device, &config);

        Ok(Self {
            window,
            device,
            queue,
            config,
            texture_bind_group_layout,
            depth_texture,
            pending_resize: None,
        })
    }

    pub fn resize(&mut self, config: wgpu::SurfaceConfiguration) {
        self.config = config;
        self.depth_texture = Texture::create_depth_texture(Some("Depth texture"), &self.device, &self.config);
    }
}
