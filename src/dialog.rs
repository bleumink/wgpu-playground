use crate::renderer::{AssetKind, AssetLoader, ResourcePath};

fn create_dialog_future() -> impl Future<Output = Option<rfd::FileHandle>> {
    rfd::AsyncFileDialog::new()
        .add_filter("Scene", AssetKind::Gltf.extensions())
        .add_filter("Pointcloud", AssetKind::Pointcloud.extensions())
        .add_filter("Environment Map", AssetKind::EnvironmentMap.extensions())
        .pick_file()
}

#[cfg(not(target_family = "wasm"))]
pub fn open_file_dialog(loader: AssetLoader) {
    use futures_lite::future;

    std::thread::spawn(move || {
        if let Some(handle) = future::block_on(create_dialog_future()) {
            loader.load(ResourcePath::new(&handle.file_name()).unwrap());
        }
    });
}

#[cfg(target_family = "wasm")]
pub fn open_file_dialog(loader: AssetLoader) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(handle) = create_dialog_future().await {
            loader.load(ResourcePath::Upload(handle.inner().clone()));
        }
    });
}
