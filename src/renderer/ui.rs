use std::sync::Arc;

use egui::{ClippedPrimitive, Context, TexturesDelta};
use egui_wgpu::ScreenDescriptor;
use egui_winit::State;
use winit::{event::WindowEvent, window::Window};

pub struct UiData {
    pub textures_delta: TexturesDelta,
    pub paint_jobs: Vec<ClippedPrimitive>,
    pub screen_descriptor: ScreenDescriptor,
}

pub struct Ui {
    window: Arc<Window>,
    context: Context,
    state: State,
    pending_resize: bool,
}

impl Ui {
    pub fn new(window: Arc<Window>) -> Self {
        let context = egui::Context::default();
        let state = State::new(context.clone(), Default::default(), &window, None, None, None);

        Self {
            window,
            context,
            state,
            pending_resize: false,
        }
    }

    pub fn on_event(&mut self, event: &WindowEvent) -> bool {
        self.state.on_window_event(&self.window, event).consumed
    }

    pub fn begin_frame(&mut self) -> &Context {
        let raw_input = self.state.take_egui_input(&self.window);
        self.context.begin_pass(raw_input);
        &self.context
    }

    pub fn end_frame(&mut self) -> Option<UiData> {
        if self.pending_resize {
            self.pending_resize = false;
            return None;
        }

        let full_output = self.context.end_pass();

        let paint_jobs = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: self.window.inner_size().into(),
            pixels_per_point: self.context.pixels_per_point(),
        };

        Some(UiData {
            textures_delta: full_output.textures_delta,
            paint_jobs,
            screen_descriptor,
        })
    }

    pub fn drop_frame(&mut self) {
        self.pending_resize = true;
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn state(&self) -> &State {
        &self.state
    }
}
