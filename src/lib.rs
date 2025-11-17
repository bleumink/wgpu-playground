use winit::event_loop::EventLoop;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

use crate::app::App;

mod app;
mod camera;
mod dialog;
mod entity;
mod error;
mod renderer;
mod state;

pub fn run() -> anyhow::Result<()> {
    #[cfg(not(target_family = "wasm"))]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,egui_wgpu=error")).init();

    #[cfg(target_family = "wasm")]
    console_log::init_with_level(log::Level::Info).unwrap_throw();

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::new(
        #[cfg(target_family = "wasm")]
        &event_loop,
    );

    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen(start)]
pub fn entry_point() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    if web_sys::window().is_some() {
        run().unwrap_throw();
    }

    Ok(())
}
