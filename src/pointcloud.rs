use std::io::Cursor;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::{asset::ResourcePath, context::RenderContext, vertex::Vertex};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PointVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

impl Vertex for PointVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PointVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },            
            ]
        }
    }
}

pub struct PointcloudBuffer(Vec<PointVertex>);

impl PointcloudBuffer {
    pub fn new(points: Vec<PointVertex>) -> Self {
        Self(points)
    }

    pub fn points(&self) -> &[PointVertex] {
        &self.0
    }

    pub fn from_las(data: Vec<u8>) -> anyhow::Result<Self> {
        // let data = path.load_binary().await?;
        let cursor = Cursor::new(data);
        let mut reader = las::Reader::new(cursor)?;

        let min_bounds = reader.header().bounds().min;
        let points: Vec<PointVertex> = reader
            .points()
            .map(|p| -> anyhow::Result<_> {
                let point = p?;
                let [x, y, z] = [
                    (point.x - min_bounds.x) as f32,
                    (point.y - min_bounds.y) as f32,
                    (point.z - min_bounds.z) as f32,
                ];

                let [r, g, b] = point
                    .color
                    .map(|color| {
                        [
                            color.red as f32 / u16::MAX as f32,
                            color.green as f32 / u16::MAX as f32,
                            color.blue as f32 / u16::MAX as f32,
                        ]
                    })
                    .unwrap_or([1.0, 1.0, 1.0]);

                let intensity = point.intensity as f32 / u16::MAX as f32;

                Ok(PointVertex {
                    position: [x, y, z],
                    color: [r, g, b],
                    intensity,
                })
            })
            .collect::<anyhow::Result<_>>()?;

        Ok(Self(points))
    }
}

pub struct Pointcloud {
    pub label: Option<String>,
    pub vertex_buffer: wgpu::Buffer,
    pub num_points: u32,
    // pub transform: [[f32; 4]; 4],
    // pub transform_buffer: wgpu::Buffer,
}

impl Pointcloud {
    pub fn from_buffer(buffer: PointcloudBuffer, context: &RenderContext, label: Option<String>) -> Self {
        let num_points = buffer.points().len() as u32;
        let vertex_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: label.as_deref(),
            contents: bytemuck::cast_slice(buffer.points()),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            label,
            vertex_buffer,
            num_points,
        }
    }
}

pub trait DrawPointcloud<'a> {
    fn draw_pointcloud(&mut self, pointcloud: &'a Pointcloud);
}

impl<'a, 'b> DrawPointcloud<'b> for wgpu::RenderPass<'a> {
    fn draw_pointcloud(&mut self, pointcloud: &'b Pointcloud) {
        self.set_vertex_buffer(0, pointcloud.vertex_buffer.slice(..));
        self.draw(0..pointcloud.num_points, 0..1);
    }
}
