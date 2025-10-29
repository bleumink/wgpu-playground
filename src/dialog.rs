use crate::asset::AssetLoader;
use crate::asset::ResourcePath;

fn create_dialog_future() -> impl Future<Output = Option<rfd::FileHandle>> {
    rfd::AsyncFileDialog::new()
        .add_filter("Scene", &["obj", "gltf", "glb"])
        .add_filter("Pointcloud", &["las", "laz"])        
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
