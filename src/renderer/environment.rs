use std::io::Cursor;

use half::f16;
use image::{ImageDecoder, codecs::hdr::HdrDecoder};

use crate::renderer::{
    context::RenderContext,
    texture::{CubeTexture, Texture},
};

pub struct IrradianceMap {
    texture: CubeTexture,
    bind_group: wgpu::BindGroup,
}

impl IrradianceMap {
    pub fn default(context: &RenderContext) -> CubeTexture {
        let data: [f16; 4] = [
            f16::from_f32(0.03), 
            f16::from_f32(0.03), 
            f16::from_f32(0.03), 
            f16::from_f32(1.0), 
        ];

        CubeTexture::create_placeholder(&context.device, &context.queue, &data, wgpu::FilterMode::Linear)     
    }

    pub fn new(environment_map: &CubeTexture, context: &RenderContext) -> CubeTexture {
        let label = Some("Irradiance map");
        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            label,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let destination = CubeTexture::create_2d_texture(
            &context.device,
            environment_map.texture().width(),
            environment_map.texture().height(),
            wgpu::TextureFormat::Rgba16Float,
            sampler,
            Some("Irradiance map"),
        );

        let dest_view = destination.texture().create_view(&wgpu::TextureViewDescriptor {
            label,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_array_layer: 0,
            array_layer_count: Some(6),
            ..Default::default()
        });

        let shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Irradiance shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../res/irradiance.wgsl").into()),
        });

        let bind_group_layout = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Irradiance bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Irradiance map pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = context.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Irradiance compute pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("irradiance_convolution"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(environment_map.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(environment_map.sampler())
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dest_view),
                },
            ],
        });

        let mut encoder = context.device.create_command_encoder(&Default::default());
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label,
                timestamp_writes: None,
            });
            let num_workgroup = (environment_map.texture().width() + 7) / 8;
            compute_pass.set_pipeline(&pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(num_workgroup, num_workgroup, 6);
        }

        context.queue.submit(Some(encoder.finish()));
        destination
    }
}

pub struct EnvironmentMap {
    environment: CubeTexture,
    irradiance: CubeTexture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl EnvironmentMap {
    pub fn default(context: &RenderContext) -> Self {
        let environment = CubeTexture::create_placeholder(&context.device, &context.queue, &[0.1f32,0.2,0.3,1.0], wgpu::FilterMode::Nearest);
        Self::new(environment, context)
    }

    pub fn new(environment: CubeTexture, context: &RenderContext) -> Self {
        let irradiance = IrradianceMap::default(context);
        let bind_group = Self::create_bind_group(&environment, &irradiance, context);

        let shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Skybox shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../res/environment.wgsl").into()),
        });

        let pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Environment map pipeline layout"),
            bind_group_layouts: &[
                &context.environment_bind_group_layout,
                &context.camera_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Environment map pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.hdr.format(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            environment,
            irradiance,
            bind_group,
            pipeline,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn compute_irradiance(&mut self, context: &RenderContext) {
        self.irradiance = IrradianceMap::new(&self.environment, context);
        self.bind_group = Self::create_bind_group(&self.environment, &self.irradiance, context)
    }

    fn create_bind_group(environment: &CubeTexture, irradiance: &CubeTexture, context: &RenderContext) -> wgpu::BindGroup {
        context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment map bind group"),
            layout: &context.environment_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(environment.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(environment.sampler()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(irradiance.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(irradiance.sampler()),
                },
            ],
        })
    }
}

pub struct HdrLoader {
    texture_format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

impl HdrLoader {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../../res/equirect.wgsl"));
        let texture_format = wgpu::TextureFormat::Rgba32Float;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HDR equirect"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: texture_format,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HDR equirect pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HDR equirect to cubemap"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("equirect_to_cubemap"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            texture_format,
            layout,
            pipeline,
        }
    }

    pub fn from_buffer(
        &self,
        buffer: HdrBuffer,
        dest_size: u32,
        label: Option<&str>,
        context: &RenderContext,
    ) -> anyhow::Result<CubeTexture> {
        let size = wgpu::Extent3d {
            width: buffer.width,
            height: buffer.height,
            depth_or_array_layers: 1,
        };

        let texture = context.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytemuck::cast_slice(&buffer.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(std::mem::size_of::<[f32; 4]>() as u32 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );

        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            label,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let source = Texture { texture, sampler, view };

        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            label,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let destination =
            CubeTexture::create_2d_texture(&context.device, dest_size, dest_size, self.texture_format, sampler, label);
        
        let dest_view = destination.texture().create_view(&wgpu::TextureViewDescriptor {
            label,
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dest_view),
                },
            ],
        });

        let mut encoder = context.device.create_command_encoder(&Default::default());

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label,
                timestamp_writes: None,
            });
            let num_workgroup = (dest_size + 15) / 16;
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(num_workgroup, num_workgroup, 6);
        }

        context.queue.submit(Some(encoder.finish()));
        Ok(destination)
    }
}

pub struct HdrBuffer {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl HdrBuffer {
    pub fn from_hdr(data: &[u8]) -> Self {
        let decoder = HdrDecoder::new(Cursor::new(data)).unwrap();
        let metadata = decoder.metadata();

        let buffer_size = (metadata.height * metadata.width) as usize * std::mem::size_of::<[f32; 3]>();
        let mut pixels = vec![0; buffer_size];
        decoder.read_image(&mut pixels).unwrap();

        let mut rgba = Vec::with_capacity(pixels.len() / 3 * 4);
        for chunk in pixels.chunks_exact(12) {
            rgba.extend_from_slice(chunk);
            rgba.extend_from_slice(&[0, 0, 128, 63]);
        }

        Self {
            pixels: rgba,
            width: metadata.width,
            height: metadata.height,
        }
    }
}
