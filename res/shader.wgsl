// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct CameraUniform {
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
}

struct TransformUniform {
    matrix: mat4x4<f32>,
}

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(2) @binding(0)
var<storage, read> transforms: array<TransformUniform>;

@group(2) @binding(1)
var<storage, read> instance_indices: array<u32>;

@vertex
fn vs_main(
    mesh: VertexInput,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let global_index = instance_indices[instance_id];
    let transform = transforms[global_index].matrix;

    var out: VertexOutput;
    out.tex_coords = mesh.tex_coords;
    out.clip_position = camera.view_projection * transform * vec4<f32>(mesh.position, 1.0);
    return out;
}

// Fragment shader
@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
}