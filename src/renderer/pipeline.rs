use std::collections::HashMap;

pub struct PipelineCache(HashMap<&'static str, wgpu::RenderPipeline>);
impl PipelineCache {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, id: &'static str, pipeline: wgpu::RenderPipeline) {
        self.0.insert(id, pipeline);
    }

    pub fn get(&self, id: &str) -> Option<&wgpu::RenderPipeline> {
        self.0.get(id)
    }
}
