use std::sync::Arc;

use winit::window::Window;

use crate::context::RenderContext;

#[derive(Debug)]
pub enum SurfaceState {
    Unconfigured,
    Configured,
    Resizing,
    Acquired(wgpu::SurfaceTexture),
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self::Unconfigured
    }
}

pub struct Surface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    state: SurfaceState,
    pending_resize: Option<(wgpu::SurfaceConfiguration, wgpu::Device)>,
}

impl Surface {
    pub async fn initialize(window: Arc<Window>) -> anyhow::Result<(Self, RenderContext)> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_family = "wasm"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_family = "wasm")]
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        log::info!("info: {:?}", adapter.get_info());

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_capabilities.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_capabilities.present_modes[0],
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![surface_format.add_srgb_suffix()],
            desired_maximum_frame_latency: 2,
        };

        let context = RenderContext::new(window, &adapter, config.clone()).await?;
        let surface_state = Self {
            surface,
            config,
            state: SurfaceState::Unconfigured,
            pending_resize: None,
        };

        Ok((surface_state, context))
    }

    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    pub fn state(&self) -> &SurfaceState {
        &self.state
    }

    pub fn acquire(&mut self) -> Result<wgpu::TextureView, wgpu::SurfaceError> {
        if let SurfaceState::Configured = self.state {
            let output = self.surface.get_current_texture()?;
            let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.config.format.add_srgb_suffix()),
                ..Default::default()
            });

            self.state = SurfaceState::Acquired(output);
            Ok(view)
        } else {
            Err(wgpu::SurfaceError::Lost)
        }
    }

    pub fn present(&mut self) {
        if let SurfaceState::Acquired(output) = std::mem::take(&mut self.state) {
            if let Some((config, device)) = self.pending_resize.take() {
                drop(output);

                self.config = config;
                self.surface.configure(&device, &self.config);
            } else {
                output.present();
            }

            self.state = SurfaceState::Configured;
        }
    }

    pub fn request_resize(&mut self, width: u32, height: u32) -> wgpu::SurfaceConfiguration {
        match &mut self.state {
            SurfaceState::Unconfigured | SurfaceState::Configured => {
                self.state = SurfaceState::Resizing;
            }
            _ => (),
        }

        let mut config = self.config.clone();
        config.width = width;
        config.height = height;
        config
    }

    pub fn apply_resize(&mut self, config: wgpu::SurfaceConfiguration, device: wgpu::Device) {
        match &mut self.state {
            SurfaceState::Acquired(_) => {
                self.pending_resize = Some((config, device));
            }
            SurfaceState::Resizing => {
                self.config = config;
                self.surface.configure(&device, &self.config);
                self.state = SurfaceState::Configured;
            }
            _ => (),
        }
    }

    pub fn drop(&mut self) {
        self.state = SurfaceState::Unconfigured;
    }
}
