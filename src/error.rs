#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Transform buffer resized")]
    ResizedTransformBuffer,
}
