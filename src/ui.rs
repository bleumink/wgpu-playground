use std::{sync::Arc, time::Duration};

use crossbeam::channel::Sender;
use egui::{ClippedPrimitive, Context, FullOutput, TexturesDelta};
use egui_wgpu::ScreenDescriptor;
use egui_winit::State;
use winit::{event::WindowEvent, window::Window};

use crate::{asset::AssetLoader, dialog::open_file_dialog, renderer::RenderCommand};

pub struct UiData {
    pub textures_delta: TexturesDelta,
    pub paint_jobs: Vec<ClippedPrimitive>,
    pub screen_descriptor: ScreenDescriptor,
}

pub struct Ui {
    window: Arc<Window>,
    context: Context,
    state: State,
    loader: AssetLoader,
}

impl Ui {
    pub fn new(window: Arc<Window>, loader: AssetLoader) -> Self {
        let context = egui::Context::default();
        let state = State::new(context.clone(), Default::default(), &window, None, None, None);

        Self {
            window,
            context,
            state,
            loader,
        }
    }

    pub fn is_input_consumed(&mut self, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(&self.window, event);
        response.consumed
    }

    pub fn build(&mut self, config: &wgpu::SurfaceConfiguration, dt: instant::Duration) -> UiData {
        let raw_input = self.state.take_egui_input(&self.window);
        self.context.begin_pass(raw_input);

        egui::TopBottomPanel::top("top_panel").show(&self.context, |ui| {
            ui.horizontal(|ui| {
                if ui.button("File").clicked() {
                    log::info!("file?");
                }
                ui.label("WGPU Speeltuin");
            });
        });

        egui::SidePanel::left("side_panel")
            .resizable(true)
            .show(&self.context, |ui| {
                ui.heading("Sidebar");
                ui.separator();
                if ui.button("Load Asset").clicked() {
                    open_file_dialog(self.loader.clone());
                }
                ui.label("Status: Ready");
            });

        // egui::CentralPanel::default()
        //     .show(&self.context, |ui| {
        //         ui.heading("Main content");
        //         ui.label("Put your scene or widgets here!");
        //     });

        egui::Window::new("Hello")
            .resizable(true)
            .movable(true)
            .show(&self.context, |ui| {
                ui.label(format!("FPS: {}", (1.0 / dt.as_secs_f32()).round()))
            });

        let full_output = self.context.end_pass();

        let paint_jobs = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [config.width, config.height],
            pixels_per_point: self.context.pixels_per_point(),
        };

        UiData {
            textures_delta: full_output.textures_delta,
            paint_jobs,
            screen_descriptor,
        }
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn state(&self) -> &State {
        &self.state
    }
}
