use std::{borrow::Cow, io::Cursor, path::Path};

use crossbeam::channel::Sender;

#[cfg(not(target_family = "wasm"))]
use futures_lite::future;
use image::{ImageDecoder, codecs::hdr::HdrDecoder};
#[cfg(not(target_family = "wasm"))]
use instant::Instant;

use serde::{Deserialize, Serialize};

#[cfg(target_family = "wasm")]
use crate::renderer::worker::{LoadTask, UploadTask, WorkerPool};

use crate::renderer::{RenderCommand, environment::HdrBuffer, mesh::SceneBuffer, pointcloud::PointcloudBuffer};

#[derive(Clone)]
pub enum ResourcePath {
    File(std::path::PathBuf),
    Url(reqwest::Url),
    #[cfg(target_family = "wasm")]
    Upload(web_sys::File),
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Serialize, Deserialize)]
pub enum SerializableResourcePath {
    File(std::path::PathBuf),
    Url(reqwest::Url),
}

impl ResourcePath {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        #[cfg(not(target_family = "wasm"))]
        return Ok(ResourcePath::File(Path::new(path).to_path_buf()));

        #[cfg(target_family = "wasm")]
        return Ok(ResourcePath::Url(format_url(path)));
    }

    #[cfg(target_family = "wasm")]
    pub fn as_serializable(&self) -> Option<SerializableResourcePath> {
        Option::<SerializableResourcePath>::from(self)
    }

    #[cfg(target_family = "wasm")]
    pub fn file(&self) -> Option<&web_sys::File> {
        match self {
            Self::File(_) | Self::Url(_) => None,
            Self::Upload(file) => Some(file),
        }
    }

    pub fn url(&self) -> Option<&reqwest::Url> {
        match self {
            Self::File(_) => None,
            Self::Url(url) => Some(url),
            #[cfg(target_family = "wasm")]
            Self::Upload(_) => None,
        }
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(path) => Some(path.as_path()),
            Self::Url(_) => None,
            #[cfg(target_family = "wasm")]
            Self::Upload(_) => None,
        }
    }

    pub fn as_str(&self) -> Cow<'_, str> {
        match self {
            Self::File(path) => match path.to_str() {
                Some(value) => Cow::Borrowed(value),
                None => Cow::Owned(path.display().to_string()),
            },
            Self::Url(url) => Cow::Borrowed(url.as_str()),
            #[cfg(target_family = "wasm")]
            Self::Upload(file) => Cow::Owned(file.name()),
        }
    }

    pub fn file_name(&self) -> Cow<'_, str> {
        match self {
            Self::File(path) => path
                .file_name()
                .and_then(|os_str| os_str.to_str())
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Owned(path.display().to_string())),
            Self::Url(url) => {
                let path = url.path();
                Path::new(path)
                    .file_name()
                    .and_then(|os_str| os_str.to_str())
                    .map(Cow::Borrowed)
                    .unwrap_or_else(|| Cow::Owned(String::new()))
            }
            #[cfg(target_family = "wasm")]
            Self::Upload(file) => Cow::Owned(file.name()),
        }
    }

    pub fn extension(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::File(path) => path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(Cow::Borrowed),
            Self::Url(url) => Path::new(url.path())
                .extension()
                .and_then(|extension| extension.to_str())
                .map(Cow::Borrowed),
            #[cfg(target_family = "wasm")]
            Self::Upload(file) => {
                let name = file.name();
                Path::new(&name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| Cow::Owned(extension.to_string()))
            }
        }
    }

    pub fn create_relative(&self, name: &str) -> Self {
        match self {
            Self::File(path) => {
                let new_path = path
                    .parent()
                    .map(|parent| parent.join(name))
                    .unwrap_or_else(|| std::path::PathBuf::from(name));
                Self::File(new_path)
            }
            Self::Url(url) => {
                let mut new_url = url.clone();
                {
                    let mut segments = new_url.path_segments_mut().expect("base URL cannot be base");
                    segments.pop_if_empty();
                    segments.pop();
                    segments.push(name);
                }

                Self::Url(new_url)
            }
            #[cfg(target_family = "wasm")]
            Self::Upload(file) => {
                let parent = file.name();
                let base = Path::new(&parent).parent().unwrap_or_else(|| Path::new(""));
                let new_name = base.join(name).display().to_string();
                Self::Url(reqwest::Url::parse(&format!("file:///{}", new_name)).unwrap())
            }
        }
    }

    pub async fn load_string(&self) -> anyhow::Result<String> {
        let text = match self {
            Self::File(path) => {
                let path_buf = std::path::Path::new(env!("OUT_DIR")).join("res").join(path);
                std::fs::read_to_string(path_buf)?
            }
            Self::Url(url) => {
                let response = reqwest::get(url.as_str()).await?;
                response.text().await?
            }
            #[cfg(target_family = "wasm")]
            Self::Upload(_) => {
                let bytes = self.load_binary().await?;
                String::from_utf8(bytes)?
            }
        };

        Ok(text)
    }

    pub async fn load_binary(&self) -> anyhow::Result<Vec<u8>> {
        let data = match self {
            Self::File(path) => {
                let path_buf = std::path::Path::new(env!("OUT_DIR")).join("res").join(path);
                std::fs::read(path_buf)?
            }
            Self::Url(url) => {
                let response = reqwest::get(url.as_str()).await?;
                response.bytes().await?.to_vec()
            }
            #[cfg(target_family = "wasm")]
            Self::Upload(file) => {
                use wasm_bindgen_futures::JsFuture;

                let buffer = JsFuture::from(file.array_buffer()).await.unwrap();
                let array = js_sys::Uint8Array::new(&buffer);

                let mut data = vec![0u8; array.length() as usize];
                array.copy_to(&mut data);
                data
            }
        };

        Ok(data)
    }
}

#[cfg(target_family = "wasm")]
impl From<&ResourcePath> for Option<SerializableResourcePath> {
    fn from(value: &ResourcePath) -> Self {
        match value {
            ResourcePath::File(path) => Some(SerializableResourcePath::File(path.clone())),
            ResourcePath::Url(url) => Some(SerializableResourcePath::Url(url.clone())),
            ResourcePath::Upload(_) => None,
        }
    }
}

#[cfg(target_family = "wasm")]
impl From<SerializableResourcePath> for ResourcePath {
    fn from(value: SerializableResourcePath) -> Self {
        match value {
            SerializableResourcePath::File(path) => ResourcePath::File(path),
            SerializableResourcePath::Url(url) => ResourcePath::Url(url),
        }
    }
}

impl std::fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub enum AssetBuffer {
    EnvironmentMap { buffer: HdrBuffer, label: Option<String> },
    Pointcloud(PointcloudBuffer, Option<String>),
    Scene(SceneBuffer, Option<String>),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AssetKind {
    Obj,
    Gltf,
    Pointcloud,
    EnvironmentMap,
}

impl AssetKind {
    pub fn to_str(&self) -> &str {
        match self {
            AssetKind::Obj => "obj",
            AssetKind::Gltf => "gltf",
            AssetKind::Pointcloud => "pointcloud",
            AssetKind::EnvironmentMap => "environment_map",
        }
    }

    pub fn from_str(kind: &str) -> Option<AssetKind> {
        match kind {
            "obj" => Some(AssetKind::Obj),
            "gltf" => Some(AssetKind::Gltf),
            "pointcloud" => Some(AssetKind::Pointcloud),
            "environment_map" => Some(AssetKind::EnvironmentMap),
            _ => None,
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        let extension = extension.to_ascii_lowercase();
        [Self::Obj, Self::Gltf, Self::Pointcloud, Self::EnvironmentMap]
            .into_iter()
            .find(|kind| kind.extensions().contains(&extension.as_str()))
    }

    pub fn extensions(&self) -> &[&'static str] {
        match self {
            AssetKind::Obj => &["obj"],
            AssetKind::Gltf => &["gltf", "glb"],
            AssetKind::Pointcloud => &["las", "laz"],
            AssetKind::EnvironmentMap => &["hdr", "exr"],
        }
    }
}

impl std::fmt::Display for AssetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

#[derive(Clone)]
pub struct AssetLoader {
    render_tx: Sender<RenderCommand>,
    #[cfg(target_family = "wasm")]
    worker_pool: WorkerPool,
}

impl AssetLoader {
    pub fn new(sender: Sender<RenderCommand>) -> Self {
        Self {
            render_tx: sender.clone(),
            #[cfg(target_family = "wasm")]
            worker_pool: WorkerPool::new(sender),
        }
    }

    pub fn load(&self, path: ResourcePath) {
        if let Some(extension) = path.extension().as_deref() {
            if let Some(kind) = AssetKind::from_extension(extension) {
                self.load_kind(kind, path);
            } else {
                log::error!("Unsupported resource");
            }
        }
    }

    fn load_kind(&self, kind: AssetKind, path: ResourcePath) {
        match kind {
            AssetKind::Obj => self.load_obj(path),
            AssetKind::Gltf => self.load_gltf(path),
            AssetKind::Pointcloud => self.load_pointcloud(path),
            AssetKind::EnvironmentMap => self.load_skybox(path),
        }
    }

    fn load_obj(&self, path: ResourcePath) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.file_name().to_string();

            std::thread::spawn(move || {
                let scene = future::block_on(SceneBuffer::from_obj(&path)).unwrap();
                sender
                    .send(RenderCommand::LoadAsset(AssetBuffer::Scene(scene, Some(filename))))
                    .unwrap();
                log::info!("Loaded {} in {} s", path.as_str(), timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        {
            match path {
                ResourcePath::File(_) | ResourcePath::Url(_) => {
                    self.worker_pool.submit(LoadTask {
                        kind: AssetKind::Obj,
                        path: path.as_serializable().unwrap(),
                    });
                }
                ResourcePath::Upload(_) => {
                    self.worker_pool.submit(UploadTask {
                        kind: AssetKind::Obj,
                        path,
                    });
                }
            };
        }
    }

    fn load_gltf(&self, path: ResourcePath) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.file_name().to_string();

            std::thread::spawn(move || {
                let data = future::block_on(path.load_binary()).unwrap();
                let scene = SceneBuffer::from_gltf(data).unwrap();
                sender
                    .send(RenderCommand::LoadAsset(AssetBuffer::Scene(scene, Some(filename))))
                    .unwrap();
                log::info!("Loaded {} in {} s", path.as_str(), timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        {
            match path {
                ResourcePath::File(_) | ResourcePath::Url(_) => {
                    self.worker_pool.submit(LoadTask {
                        kind: AssetKind::Gltf,
                        path: path.as_serializable().unwrap(),
                    });
                }
                ResourcePath::Upload(_) => {
                    self.worker_pool.submit(UploadTask {
                        kind: AssetKind::Gltf,
                        path,
                    });
                }
            };
        }
    }

    fn load_pointcloud(&self, path: ResourcePath) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.file_name().to_string();

            std::thread::spawn(move || {
                let data = future::block_on(path.load_binary()).unwrap();
                let pointcloud = PointcloudBuffer::from_las(data).unwrap();
                sender
                    .send(RenderCommand::LoadAsset(AssetBuffer::Pointcloud(
                        pointcloud,
                        Some(filename),
                    )))
                    .unwrap();
                log::info!("Loaded {} in {} s", path, timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        {
            match path {
                ResourcePath::File(_) | ResourcePath::Url(_) => {
                    self.worker_pool.submit(LoadTask {
                        kind: AssetKind::Pointcloud,
                        path: path.as_serializable().unwrap(),
                    });
                }
                ResourcePath::Upload(_) => {
                    self.worker_pool.submit(UploadTask {
                        kind: AssetKind::Pointcloud,
                        path,
                    });
                }
            };
        }
    }

    fn load_skybox(&self, path: ResourcePath) {
        #[cfg(not(target_family = "wasm"))]
        {
            let sender = self.render_tx.clone();
            let timestamp = Instant::now();
            let filename = path.file_name().to_string();

            std::thread::spawn(move || {
                use crate::renderer::environment::HdrBuffer;

                let data = future::block_on(path.load_binary()).unwrap();
                let buffer = HdrBuffer::from_hdr(&data);

                sender
                    .send(RenderCommand::LoadAsset(AssetBuffer::EnvironmentMap {
                        buffer,
                        label: Some(filename),
                    }))
                    .unwrap();
                log::info!("Loaded {} in {} s", path, timestamp.elapsed().as_secs_f32());
            });
        }

        #[cfg(target_family = "wasm")]
        {
            match path {
                ResourcePath::File(_) | ResourcePath::Url(_) => {
                    self.worker_pool.submit(LoadTask {
                        kind: AssetKind::EnvironmentMap,
                        path: path.as_serializable().unwrap(),
                    });
                }
                ResourcePath::Upload(_) => {
                    self.worker_pool.submit(UploadTask {
                        kind: AssetKind::EnvironmentMap,
                        path,
                    });
                }
            };
        }
    }
}

#[cfg(target_family = "wasm")]
fn format_url(filename: &str) -> reqwest::Url {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let mut origin = location.origin().unwrap();
    if !origin.ends_with("res") {
        origin = format!("{}/res", origin);
    }

    let base = reqwest::Url::parse(&format!("{}/", origin)).unwrap();
    base.join(filename).unwrap()
}
