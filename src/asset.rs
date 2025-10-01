use std::{borrow::Cow, path::Path};

use crossbeam::channel::Sender;

#[cfg(not(target_family = "wasm"))]
use futures_lite::future;
#[cfg(not(target_family = "wasm"))]
use instant::Instant;

use serde::{Deserialize, Serialize};

#[cfg(target_family = "wasm")]
use crate::worker::{Task, WorkerPool};

use crate::{instance::Instance, model::ModelBuffer, pointcloud::PointcloudBuffer, renderer::RenderEvent};

#[derive(Clone, Serialize, Deserialize)]
pub enum ResourcePath {
    File(String),
    Url(String),
}

impl ResourcePath {
    pub fn new(path: &str) -> ResourcePath {
        #[cfg(not(target_family = "wasm"))]
        return ResourcePath::File(path.to_string());

        #[cfg(target_family = "wasm")]
        return ResourcePath::Url(format_url(path));
    }

    pub fn as_str(&self) -> &str {
        match self {
            ResourcePath::File(path) | ResourcePath::Url(path) => path.as_str(),
        }
    }

    pub fn extension(&self) -> Option<&str> {
        match self {
            ResourcePath::File(path) | ResourcePath::Url(path) => {
                Path::new(path).extension().and_then(|extension| extension.to_str())
            }
        }
    }

    pub fn filename(&self) -> Cow<'_, str> {
        match self {
            ResourcePath::File(path) | ResourcePath::Url(path) => {
                Path::new(path).file_name().unwrap().to_string_lossy()
            }
        }
    }

    pub fn create_relative(&self, path: &str) -> ResourcePath {
        let relative_path = match self {
            ResourcePath::File(p) | ResourcePath::Url(p) => match Path::new(p).parent() {
                Some(parent) => parent.join(path),
                None => Path::new(".").join(path),
            },
        }
        .to_string_lossy()
        .to_string();

        match self {
            ResourcePath::File(_) => ResourcePath::File(relative_path),
            ResourcePath::Url(_) => ResourcePath::Url(relative_path),
        }
    }

    pub async fn load_string(&self) -> anyhow::Result<String> {
        let text = match self {
            ResourcePath::File(path) => {
                let path_buf = std::path::Path::new(env!("OUT_DIR")).join("res").join(path);
                std::fs::read_to_string(path_buf)?
            }
            ResourcePath::Url(url) => {
                let response = reqwest::get(url).await?;
                response.text().await?
            }
        };

        Ok(text)
    }

    pub async fn load_binary(&self) -> anyhow::Result<Vec<u8>> {
        let data = match self {
            ResourcePath::File(path) => {
                let path_buf = std::path::Path::new(env!("OUT_DIR")).join("res").join(path);
                std::fs::read(path_buf)?
            }
            ResourcePath::Url(url) => {
                let response = reqwest::get(url).await?;
                response.bytes().await?.to_vec()
            }
        };

        Ok(data)
    }
}

impl std::fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub enum Asset {
    Pointcloud(PointcloudBuffer, Option<String>, Option<Vec<LoadOptions>>),
    Model(ModelBuffer, Option<String>, Option<Vec<LoadOptions>>),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum LoadOptions {
    Transform(glam::Mat4),
    Instanced(Vec<Instance>),
}

pub struct AssetLoader {
    render_tx: Sender<RenderEvent>,
    #[cfg(target_family = "wasm")]
    worker_pool: WorkerPool,
}

impl AssetLoader {
    pub fn new(sender: Sender<RenderEvent>) -> Self {
        Self {
            render_tx: sender.clone(),
            #[cfg(target_family = "wasm")]
            worker_pool: WorkerPool::new(sender),
        }
    }

    pub fn load(&self, path: ResourcePath, options: Option<Vec<LoadOptions>>) {
        match path.extension() {
            Some("obj") => self.load_model(path, options),
            Some("las") | Some("laz") => self.load_pointcloud(path, options),
            _ => (),
        }
    }

    fn load_model(&self, path: ResourcePath, options: Option<Vec<LoadOptions>>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.filename().to_string();

            std::thread::spawn(move || {
                let model = future::block_on(ModelBuffer::from_obj(&path)).unwrap();
                sender
                    .send(RenderEvent::LoadAsset(Asset::Model(model, Some(filename), options)))
                    .unwrap();
                log::info!("Loaded {} in {} s", path.as_str(), timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        self.worker_pool.submit_task(Task::LoadModel(path, options));
    }

    fn load_pointcloud(&self, path: ResourcePath, options: Option<Vec<LoadOptions>>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.filename().to_string();

            std::thread::spawn(move || {
                let pointcloud = future::block_on(PointcloudBuffer::from_las(&path)).unwrap();
                sender
                    .send(RenderEvent::LoadAsset(Asset::Pointcloud(
                        pointcloud,
                        Some(filename),
                        options,
                    )))
                    .unwrap();
                log::info!("Loaded {} in {} s", path.as_str(), timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        self.worker_pool.submit_task(Task::LoadPointcloud(path, options));
    }
}

#[cfg(target_family = "wasm")]
fn format_url(filename: &str) -> String {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let mut origin = location.origin().unwrap();
    if !origin.ends_with("res") {
        origin = format!("{}/res", origin);
    }

    let base = reqwest::Url::parse(&format!("{}/", origin)).unwrap();
    base.join(filename).unwrap().to_string()
}
