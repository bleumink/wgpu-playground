// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) intensity: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

struct CameraUniform {
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
}

struct TransformUniform {
    matrix: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<storage, read> transforms: array<TransformUniform>;

@group(1) @binding(1)
var<storage, read> instance_indices: array<u32>;

@vertex
fn vs_main(
    points: VertexInput,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let global_index = instance_indices[instance_id];
    let transform = transforms[global_index].matrix;

    var out: VertexOutput;
    out.clip_position = camera.view_projection * transform * vec4<f32>(points.position, 1.0);
    out.color = points.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);    
}