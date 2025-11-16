use bytemuck::{Pod, Zeroable};

use crate::renderer::{context::RenderContext, transform::TransformUniform};

pub struct LightId(pub usize);

#[derive(Clone, Debug)]
pub enum Light {
    Directional {
        direction: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
    },
    Point {
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
    },
    Spot {
        position: glam::Vec3,
        direction: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        cutoff: f32,
    },
}

impl Light {
    pub fn to_light_uniform(&self) -> LightUniform {
        match self {
            Self::Directional { color, intensity, .. } => LightUniform {
                color: color.to_array(),
                kind: 0,
                intensity: *intensity,
                cutoff: 0.0,
                _padding: [0; 2],
            },
            Self::Point { color, intensity, .. } => LightUniform {
                color: color.to_array(),
                kind: 1,
                intensity: *intensity,
                cutoff: 0.0,
                _padding: [0; 2],
            },
            Self::Spot {
                color,
                intensity,
                cutoff,
                ..
            } => LightUniform {
                color: color.to_array(),
                kind: 2,
                intensity: *intensity,
                cutoff: *cutoff,
                _padding: [0; 2],
            },
        }
    }

    pub fn to_transform(&self) -> glam::Mat4 {
        fn look_dir(position: glam::Vec3, direction: glam::Vec3) -> glam::Mat4 {
            let dir = direction.normalize();
            let up = if dir.abs_diff_eq(glam::Vec3::Y, 1e-3) {
                glam::Vec3::Z
            } else {
                glam::Vec3::Y
            };

            let right = dir.cross(up).normalize();
            let up = right.cross(dir).normalize();

            glam::Mat4::from_cols(
                right.extend(0.0),
                up.extend(0.0),
                (-dir).extend(0.0),
                position.extend(1.0),
            )
        }

        match self {
            Self::Directional { direction, .. } => look_dir(glam::Vec3::ZERO, *direction),
            Self::Point { position, .. } => glam::Mat4::from_translation(*position),
            Self::Spot {
                position, direction, ..
            } => look_dir(*position, *direction),
        }
    }

    pub fn to_transform_uniform(&self) -> TransformUniform {
        TransformUniform::new(self.to_transform())
    }

    pub fn to_parts(self) -> (LightUniform, TransformUniform) {
        (self.to_light_uniform(), self.to_transform_uniform())
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct LightUniform {
    pub color: [f32; 3],
    pub cutoff: f32,
    pub intensity: f32,
    pub kind: u32,
    _padding: [u32; 2],
}
