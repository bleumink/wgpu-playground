use std::{cell::OnceCell, rc::Rc, sync::Arc};

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{renderer::Renderer, state::State, surface::Surface};

#[cfg(target_family = "wasm")]
fn get_canvas(canvas_id: &str) -> web_sys::HtmlCanvasElement {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().unwrap_throw();
    let document = window.document().unwrap_throw();
    let canvas = document.get_element_by_id(canvas_id).unwrap_throw();
    canvas.unchecked_into()
}

pub struct App {
    #[cfg(target_family = "wasm")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    state: Option<State>,
}

impl App {
    pub fn new(#[cfg(target_family = "wasm")] event_loop: &winit::event_loop::EventLoop<State>) -> Self {
        #[cfg(target_family = "wasm")]
        let proxy = Some(event_loop.create_proxy());

        Self {
            state: None,
            #[cfg(target_family = "wasm")]
            proxy,
        }
    }
}

impl ApplicationHandler<State> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_family = "wasm")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;

            let canvas = get_canvas("canvas");
            // let canvas_clone = canvas.clone();
            // let click_listener = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
            //     canvas_clone.request_pointer_lock();
            // }) as Box<dyn FnMut(_)>);

            // canvas.add_event_listener_with_callback("click", click_listener.as_ref().unchecked_ref()).unwrap_throw();
            // click_listener.forget();

            window_attributes = window_attributes.with_canvas(Some(canvas));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        let (render_tx, render_rx) = crossbeam::channel::unbounded();
        let (result_tx, result_rx) = crossbeam::channel::unbounded();

        #[cfg(not(target_family = "wasm"))]
        {
            use futures_lite::future;
            use winit::dpi::LogicalSize;

            let monitor = window.current_monitor().unwrap();
            let size = monitor.size();
            let scale = 0.50;
            let target_size = LogicalSize::new(size.width as f64 * scale, size.height as f64 * scale);
            let _ = window.request_inner_size(target_size);

            let (surface, context) = future::block_on(Surface::initialize(Arc::clone(&window))).unwrap();
            let mut renderer = future::block_on(Renderer::new(context, render_rx, result_tx)).unwrap();

            std::thread::spawn(move || {
                if let Err(error) = renderer.run() {
                    log::error!("Renderer encountered an error: {}", error);
                }
            });

            let state = future::block_on(State::new(window, surface, render_tx, result_rx)).unwrap();
            self.state = Some(state);
        }

        #[cfg(target_family = "wasm")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    let (surface, context) = Surface::initialize(Arc::clone(&window))
                        .await
                        .expect("Unable to initialize surface");

                    let renderer = Renderer::new(context, render_rx, result_tx)
                        .await
                        .expect("Unable to create renderer");

                    assert!(
                        proxy
                            .send_event(
                                State::new(window, surface, render_tx, result_rx, renderer)
                                    .await
                                    .expect("Unable to create canvas")
                            )
                            .is_ok()
                    )
                });
            }
        }

        // window.set_cursor_visible(false);
        // let cursor_grab_mode = if cfg!(target_os = "macos") | cfg!(target_family = "wasm") {
        //     CursorGrabMode::Locked
        // } else {
        //     CursorGrabMode::Confined
        // };
        // window.set_cursor_grab(cursor_grab_mode).unwrap();
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: State) {
        #[cfg(target_family = "wasm")]
        {
            event.window().request_redraw();
            event.resize(event.window().inner_size().width, event.window().inner_size().height);
        }

        self.state = Some(event);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };

        match event {
            DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                let controller = state.camera_controller_mut();

                if controller.is_mouse_pressed() {
                    controller.handle_mouse(-dx, dy);
                }
            }
            _ => (),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };

        if state.ui_mut().is_input_consumed(&event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => state.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => state.update(event_loop),
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                state
                    .camera_controller_mut()
                    .handle_mouse_button(button, button_state.is_pressed());
            }
            WindowEvent::MouseWheel { delta, .. } => {
                state.camera_controller_mut().handle_scroll(&delta);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                // TODO Move elsewhere
                if code == KeyCode::Escape && key_state.is_pressed() {
                    state.exit();
                } else {
                    state.camera_controller_mut().handle_key(code, key_state);
                    // self.handle_key(event_loop, code, key_state.is_pressed())
                }
            }
            _ => (),
        }
    }
}
