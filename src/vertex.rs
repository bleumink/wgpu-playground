pub trait Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static>;
}

pub struct VertexLayoutBuilder {
    layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    location: u32,
}

impl VertexLayoutBuilder {
    pub fn new() -> Self {
        Self {
            location: 0,
            layouts: Vec::new(),
        }
    }

    pub fn push<V: Vertex>(mut self) -> Self {
        let layout = V::desc();
        let mut attributes = layout.attributes.to_vec();

        for attribute in attributes.iter_mut() {
            attribute.shader_location += self.location;
        }

        let max_location = attributes.iter().map(|attr| attr.shader_location).max().unwrap_or(0);
        self.location = max_location + 1;

        let leaked_attributes: &'static [wgpu::VertexAttribute] = Box::leak(attributes.into_boxed_slice());
        let new_layout = wgpu::VertexBufferLayout {
            array_stride: layout.array_stride,
            step_mode: layout.step_mode,
            attributes: leaked_attributes,
        };

        self.layouts.push(new_layout);
        self
    }

    pub fn build(self) -> Vec<wgpu::VertexBufferLayout<'static>> {
        self.layouts
    }
}
