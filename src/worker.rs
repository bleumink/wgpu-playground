use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use crossbeam::channel::Sender;
use instant::Instant;
use serde::Deserialize;
use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::DedicatedWorkerGlobalScope;

use crate::asset::Asset;
use crate::asset::ResourcePath;
use crate::model::ModelBuffer;
use crate::pointcloud::PointcloudBuffer;
use crate::renderer::RenderCommand;

macro_rules! js_object {
    ({ $($key:literal : $value:expr),* $(,)? }) => {{
        let obj = js_sys::Object::new();
        $(
            js_sys::Reflect::set(&obj, &wasm_bindgen::JsValue::from_str($key), &$value)
                .expect("failed to set object property");
        )*
        obj
    }};
}

#[wasm_bindgen]
pub fn init_worker() {
    let mut runtime = WorkerRuntime::new();
    runtime.register::<LoadTask>();
    runtime.register::<UploadTask>();
    runtime.run();
}
pub struct WorkerRuntime {
    handlers: HashMap<&'static str, Rc<dyn Fn(JsValue, DedicatedWorkerGlobalScope)>>,
}

impl WorkerRuntime {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<T: WorkerTask>(&mut self) {
        let func = Rc::new(move |payload: JsValue, scope: DedicatedWorkerGlobalScope| {
            wasm_bindgen_futures::spawn_local(async move {
                let task = T::from_message(payload);
                task.run(&scope).await;
            });
        });

        self.handlers.insert(T::HANDLE, func);
    }

    pub fn run(self) {
        let onmessage = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let scope = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();

            let data = event.data();
            let task_type = js_sys::Reflect::get(&data, &"type".into())
                .unwrap()
                .as_string()
                .unwrap();
            let payload = js_sys::Reflect::get(&data, &"payload".into()).unwrap();

            if let Some(handler) = self.handlers.get(&task_type.as_str()) {
                handler(payload, scope);
            }
        }) as Box<dyn FnMut(_)>);

        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        global.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        global.post_message(&JsValue::from_str("ready")).unwrap();

        onmessage.forget();
    }
}

pub trait WorkerTask: 'static {
    const HANDLE: &'static str;

    fn from_message(payload: JsValue) -> Self;
    fn to_message(&self) -> JsValue;
    fn run(self, scope: &DedicatedWorkerGlobalScope) -> impl Future<Output = ()>;
    fn on_complete(&self, result: JsValue, sender: Sender<RenderCommand>, duration: Duration);

    fn boxed(self) -> Box<dyn AnyTask>
    where
        Self: Sized,
    {
        Box::new(self)
    }
}

pub trait AnyTask {
    fn handle(&self) -> &'static str;
    fn to_message(&self) -> JsValue;
    fn on_complete(&self, result: JsValue, sender: Sender<RenderCommand>, duration: Duration);
}

impl<T: WorkerTask> AnyTask for T {
    fn handle(&self) -> &'static str {
        T::HANDLE
    }

    fn to_message(&self) -> JsValue {
        self.to_message()
    }

    fn on_complete(&self, result: JsValue, sender: Sender<RenderCommand>, duration: Duration) {
        self.on_complete(result, sender, duration);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AssetKind {
    Model,
    Pointcloud,
}

impl AssetKind {
    fn to_str(&self) -> &str {
        match self {
            AssetKind::Model => "model",
            AssetKind::Pointcloud => "pointcloud",
        }
    }

    fn from_str(kind: &str) -> Option<AssetKind> {
        match kind {
            "model" => Some(AssetKind::Model),
            "pointcloud" => Some(AssetKind::Pointcloud),
            _ => None,
        }
    }
}

impl std::fmt::Display for AssetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

#[derive(Serialize, Deserialize)]
pub struct LoadTask {
    pub kind: AssetKind,
    pub path: ResourcePath,
}

impl WorkerTask for LoadTask {
    const HANDLE: &'static str = "load";

    fn from_message(payload: JsValue) -> Self {
        serde_wasm_bindgen::from_value::<Self>(payload).unwrap()
    }

    fn to_message(&self) -> JsValue {
        let object = js_object!({
            "type": JsValue::from_str(self.handle()),
            "payload": serde_wasm_bindgen::to_value(&self).unwrap(),
        });

        object.into()
    }

    async fn run(self, scope: &DedicatedWorkerGlobalScope) {
        let buffer = match self.kind {
            AssetKind::Model => {
                let model = ModelBuffer::from_obj(&self.path).await.unwrap();
                let raw = model.buffer();
                js_sys::Uint8Array::new_from_slice(raw).buffer()
            }
            AssetKind::Pointcloud => {
                let data = self.path.load_binary().await.unwrap();
                let pointcloud = PointcloudBuffer::from_las(data).unwrap();
                let raw = bytemuck::cast_slice(pointcloud.points());
                js_sys::Uint8Array::new_from_slice(raw).buffer()
            }
        };

        scope
            .post_message_with_transfer(&buffer, &js_sys::Array::of1(&buffer))
            .unwrap();
    }

    fn on_complete(&self, result: JsValue, sender: Sender<RenderCommand>, duration: Duration) {
        let array = js_sys::Uint8Array::new(&result);
        let mut bytes = vec![0u8; array.length() as usize];
        array.copy_to(&mut bytes);

        let filename = self.path.filename().to_string();
        match self.kind {
            AssetKind::Model => {
                let model = ModelBuffer::from_bytes(&bytes);
                sender
                    .send(RenderCommand::LoadAsset(Asset::Model(model, Some(filename.clone()))))
                    .unwrap();
            }
            AssetKind::Pointcloud => {
                let points = bytemuck::cast_slice(&bytes);
                let pointcloud = PointcloudBuffer::new(points.to_vec());
                sender
                    .send(RenderCommand::LoadAsset(Asset::Pointcloud(
                        pointcloud,
                        Some(filename.clone()),
                    )))
                    .unwrap();
            }
        }

        log::info!("Loaded {} in {} s", filename, duration.as_secs_f32());
    }
}

pub struct UploadTask {
    pub kind: AssetKind,
    pub file: web_sys::File,
}

impl WorkerTask for UploadTask {
    const HANDLE: &'static str = "upload";

    fn from_message(payload: JsValue) -> Self {
        let kind_str = js_sys::Reflect::get(&payload, &"kind".into())
            .unwrap()
            .as_string()
            .unwrap();
        let kind = AssetKind::from_str(&kind_str).unwrap();

        let file: web_sys::File = js_sys::Reflect::get(&payload, &"file".into()).unwrap().unchecked_into();

        Self { file, kind }
    }

    fn to_message(&self) -> JsValue {
        let payload = js_object!({
            "file": self.file.value_of(),
            "kind": JsValue::from_str(self.kind.to_str()),
        });

        let object = js_object!({
            "type": JsValue::from_str(self.handle()),
            "payload": payload,
        });

        object.into()
    }

    async fn run(self, scope: &DedicatedWorkerGlobalScope) {
        let buffer = JsFuture::from(self.file.array_buffer()).await.unwrap();
        let data = js_sys::Uint8Array::new(&buffer);
        let buffer = match self.kind {
            AssetKind::Model => {
                todo!();
                // let model = ModelBuffer::from_obj(data).await.unwrap();
                // model.buffer()
            }
            AssetKind::Pointcloud => {
                let mut bytes = vec![0u8; data.length() as usize];
                data.copy_to(&mut bytes);

                let pointcloud = PointcloudBuffer::from_las(bytes).unwrap();
                let raw = bytemuck::cast_slice(pointcloud.points());
                js_sys::Uint8Array::new_from_slice(raw).buffer()
            }
        };

        scope
            .post_message_with_transfer(&buffer, &js_sys::Array::of1(&buffer))
            .unwrap();
    }

    fn on_complete(&self, result: JsValue, sender: Sender<RenderCommand>, duration: Duration) {
        let array = js_sys::Uint8Array::new(&result);
        let mut bytes = vec![0u8; array.length() as usize];
        array.copy_to(&mut bytes);

        match self.kind {
            AssetKind::Model => {
                let model = ModelBuffer::from_bytes(&bytes);
                sender
                    .send(RenderCommand::LoadAsset(Asset::Model(model, Some(self.file.name()))))
                    .unwrap();
            }
            AssetKind::Pointcloud => {
                let points = bytemuck::cast_slice(&bytes);
                let pointcloud = PointcloudBuffer::new(points.to_vec());
                sender
                    .send(RenderCommand::LoadAsset(Asset::Pointcloud(
                        pointcloud,
                        Some(self.file.name()),
                    )))
                    .unwrap();
            }
        }

        log::info!("Loaded {} in {} s", self.file.name(), duration.as_secs_f32());
    }
}

struct Submission {
    task: Box<dyn AnyTask>,
    start: Instant,
}

pub enum WorkerState {
    Ready,
    Busy,
}

pub struct Worker {
    id: usize,
    state: WorkerState,
    inner: web_sys::Worker,
}

impl Worker {
    pub fn new(id: usize, pool: &Rc<RefCell<WorkerPoolInner>>) -> Self {
        let inner = {
            let opts = web_sys::WorkerOptions::new();
            opts.set_type(web_sys::WorkerType::Module);
            web_sys::Worker::new_with_options("worker.js", &opts).unwrap()
        };

        let pool_ref = Rc::downgrade(pool);
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            if let Some(pool_rc) = pool_ref.upgrade() {
                let mut pool = pool_rc.borrow_mut();
                pool.handle_message(id, event.data());
            }
        }) as Box<dyn FnMut(_)>);

        inner.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        Self {
            id,
            state: WorkerState::Busy,
            inner,
        }
    }

    pub fn post_message(&self, message: &JsValue) {
        self.inner.post_message(message).unwrap();
    }
}

#[derive(Clone)]
pub struct WorkerPool {
    inner: Rc<RefCell<WorkerPoolInner>>,
}

impl WorkerPool {
    pub fn new(sender: Sender<RenderCommand>) -> Self {
        let capacity = web_sys::window().unwrap().navigator().hardware_concurrency();
        let inner = WorkerPoolInner {
            workers: Vec::new(),
            queue: VecDeque::new(),
            capacity: capacity as usize,
            render_tx: sender,
            submissions: HashMap::new(),
        };

        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn submit<T>(&self, task: T)
    where
        T: WorkerTask,
    {
        let mut pool = self.inner.borrow_mut();
        if let Some(worker) = pool.workers.iter_mut().find(|w| matches!(w.state, WorkerState::Ready)) {
            let worker_id = worker.id;
            pool.assign_task(worker_id, task.boxed());
            return;
        }
        
        pool.queue.push_back(task.boxed());

        if pool.workers.len() < pool.capacity {
            let id = pool.workers.len();
            let worker = Worker::new(id, &self.inner);
            pool.workers.push(worker);
        }
    }
}

pub struct WorkerPoolInner {
    workers: Vec<Worker>,
    queue: VecDeque<Box<dyn AnyTask>>,
    capacity: usize,
    render_tx: Sender<RenderCommand>,
    submissions: HashMap<usize, Submission>,
}

impl WorkerPoolInner {
    pub fn handle_message(&mut self, worker_id: usize, data: JsValue) {
        if let Some(message) = data.as_string() {
            if message == "ready" {
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    worker.state = WorkerState::Ready;
                }
                self.dispatch_next();
                return;
            }
        }

        if let Some(submission) = self.submissions.remove(&worker_id) {
            let duration = submission.start.elapsed();
            submission.task.on_complete(data, self.render_tx.clone(), duration);
        }

        if let Some(worker) = self.workers.get_mut(worker_id) {
            worker.state = WorkerState::Ready;
        }

        self.dispatch_next();
    }

    fn dispatch_next(&mut self) {
        if let Some(next_task) = self.queue.pop_front() {
            if let Some(worker) = self.workers.iter_mut().find(|w| matches!(w.state, WorkerState::Ready)) {
                let worker_id = worker.id;
                self.assign_task(worker_id, next_task);
            } else {
                self.queue.push_front(next_task);
            }
        }
    }

    fn assign_task(&mut self, worker_id: usize, task: Box<dyn AnyTask>) {
        let message = task.to_message();

        let worker = &mut self.workers[worker_id];
        worker.post_message(&message);
        worker.state = WorkerState::Busy;

        self.submissions.insert(
            worker_id,
            Submission {
                task,
                start: Instant::now(),
            },
        );
    }
}
