use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::rc::Weak;
use std::time::Duration;

use crossbeam::channel::Sender;
use instant::Instant;
use serde::Deserialize;
use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::DedicatedWorkerGlobalScope;

use crate::asset::Asset;
use crate::asset::LoadOptions;
use crate::asset::ResourcePath;
use crate::model::ModelBuffer;
use crate::pointcloud::PointcloudBuffer;
use crate::renderer::RenderEvent;

#[derive(Clone, Serialize, Deserialize)]
pub enum Task {
    LoadPointcloud(ResourcePath, Option<Vec<LoadOptions>>),
    LoadModel(ResourcePath, Option<Vec<LoadOptions>>),
}

impl Task {
    pub fn path(&self) -> String {
        match self {
            Task::LoadPointcloud(path, _) => path.to_string(),
            Task::LoadModel(path, _) => path.to_string(),
        }
    }

    fn to_array_buffer(bytes: &[u8]) -> js_sys::ArrayBuffer {
        let buffer = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        buffer.copy_from(bytes);
        buffer.buffer()
    }

    fn from_array_buffer(buffer: js_sys::ArrayBuffer) -> Vec<u8> {
        let u8_array = js_sys::Uint8Array::new(&buffer);
        let mut bytes = vec![0u8; u8_array.length() as usize];
        u8_array.copy_to(&mut bytes);

        bytes
    }

    pub fn run(self, scope: DedicatedWorkerGlobalScope) {
        match self {
            Task::LoadPointcloud(path, _) => {
                wasm_bindgen_futures::spawn_local(async move {
                    let pointcloud = PointcloudBuffer::from_las(&path).await.unwrap();
                    let raw: &[u8] = bytemuck::cast_slice(pointcloud.points());
                    let array_buffer = Task::to_array_buffer(raw);

                    scope
                        .post_message_with_transfer(&array_buffer, &js_sys::Array::of1(&array_buffer))
                        .unwrap();
                });
            }
            Task::LoadModel(path, _) => {
                wasm_bindgen_futures::spawn_local(async move {
                    let model = ModelBuffer::from_obj(&path).await.unwrap();
                    let raw = model.buffer();
                    let array_buffer = Task::to_array_buffer(raw);

                    scope
                        .post_message_with_transfer(&array_buffer, &js_sys::Array::of1(&array_buffer))
                        .unwrap();
                });
            }
        }
    }

    pub fn on_complete(self, result: JsValue, sender: Sender<RenderEvent>, duration: Duration) {
        match self {
            Task::LoadPointcloud(path, options) => {
                let array_buffer: js_sys::ArrayBuffer = result.dyn_into().unwrap();
                let bytes = Task::from_array_buffer(array_buffer);
                let points = bytemuck::cast_slice(&bytes);
                let pointcloud = PointcloudBuffer::new(points.to_vec());

                let filename = path.filename().to_string();
                sender
                    .send(RenderEvent::LoadAsset(Asset::Pointcloud(
                        pointcloud,
                        Some(filename),
                        options,
                    )))
                    .unwrap();

                log::info!("Loaded {} in {} s", path, duration.as_secs_f32());
            }
            Task::LoadModel(path, options) => {
                let array_buffer: js_sys::ArrayBuffer = result.dyn_into().unwrap();
                let bytes = Task::from_array_buffer(array_buffer);
                let model = ModelBuffer::from_bytes(&bytes);

                let filename = path.filename().to_string();
                sender
                    .send(RenderEvent::LoadAsset(Asset::Model(model, Some(filename), options)))
                    .unwrap();

                log::info!("Loaded {} in {} s", path, duration.as_secs_f32());
            }
        }
    }
}

pub enum WorkerState {
    Initializing,
    Ready,
    Busy,
}

pub struct Worker {
    id: usize,
    state: WorkerState,
    inner: web_sys::Worker,
    on_message: RefCell<Closure<dyn FnMut(web_sys::MessageEvent)>>,
    pool: Weak<RefCell<WorkerPoolInner>>,
}

impl Worker {
    pub fn new(id: usize, pool: Weak<RefCell<WorkerPoolInner>>) -> Self {
        let inner = {
            let opts = web_sys::WorkerOptions::new();
            opts.set_type(web_sys::WorkerType::Module);
            web_sys::Worker::new_with_options("worker.js", &opts).unwrap()
        };

        let closure_pool = Weak::clone(&pool);
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            if let Some(message) = event.data().as_string() {
                if message == "ready" {
                    if let Some(pool_ref) = closure_pool.upgrade() {
                        pool_ref.borrow_mut().on_worker_ready(id);
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        inner.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        Self {
            id,
            state: WorkerState::Initializing,
            inner,
            on_message: RefCell::new(on_message),
            pool,
        }
    }

    pub fn run_task(&self, task: Task) {
        let timestamp = Instant::now();
        let id = self.id;

        let closure_pool = Weak::clone(&self.pool);
        let closure_task = task.clone();

        *self.on_message.borrow_mut() = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            if let Some(pool_ref) = closure_pool.upgrade() {
                pool_ref
                    .borrow_mut()
                    .on_task_done(id, closure_task.clone(), event.data(), timestamp.elapsed());
            }
        }) as Box<dyn FnMut(_)>);
        self.inner
            .set_onmessage(Some(self.on_message.borrow().as_ref().unchecked_ref()));

        let payload = serde_wasm_bindgen::to_value(&task).unwrap();
        self.inner.post_message(&payload).unwrap();
    }
}

pub struct WorkerPool {
    inner: Rc<RefCell<WorkerPoolInner>>,
}

pub struct WorkerPoolInner {
    workers: Vec<Worker>,
    queue: VecDeque<Task>,
    capacity: usize,
    render_tx: Sender<RenderEvent>,
}

impl WorkerPoolInner {
    pub fn on_worker_ready(&mut self, id: usize) {
        let worker = &mut self.workers[id];
        if let Some(task) = self.queue.pop_front() {
            worker.state = WorkerState::Busy;
            worker.run_task(task);
        } else {
            worker.state = WorkerState::Ready;
        }
    }

    pub fn on_task_done(&mut self, id: usize, task: Task, result: JsValue, duration: Duration) {
        task.on_complete(result, self.render_tx.clone(), duration);
        let worker = &mut self.workers[id];
        if let Some(task) = self.queue.pop_front() {
            worker.run_task(task);
        } else {
            worker.state = WorkerState::Ready;
        }
    }
}

impl WorkerPool {
    pub fn new(sender: Sender<RenderEvent>) -> Self {
        let capacity = web_sys::window().unwrap().navigator().hardware_concurrency();

        let inner = WorkerPoolInner {
            workers: Vec::new(),
            queue: VecDeque::new(),
            capacity: capacity as usize,
            render_tx: sender,
        };

        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn submit_task(&self, task: Task) {
        let inner = &mut self.inner.borrow_mut();
        if let Some(worker) = inner.workers.iter_mut().find(|w| matches!(w.state, WorkerState::Ready)) {
            worker.state = WorkerState::Busy;
            worker.run_task(task);
            return;
        }

        inner.queue.push_back(task);
        if inner.workers.len() < inner.capacity {
            let id = inner.workers.len();

            let worker = Worker::new(id, Rc::downgrade(&self.inner));
            inner.workers.push(worker);
        }
    }
}

#[wasm_bindgen]
pub fn init_worker() -> Result<(), wasm_bindgen::JsValue> {
    let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let scope = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        let task = serde_wasm_bindgen::from_value::<Task>(event.data()).unwrap();

        task.run(scope);
    }) as Box<dyn FnMut(_)>);

    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();

    global.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    global.post_message(&JsValue::from_str("ready")).unwrap();

    onmessage.forget();
    Ok(())
}
