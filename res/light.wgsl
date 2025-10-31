
// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

struct CameraUniform {
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
}

struct TransformUniform {
    matrix: mat4x4<f32>,
}

struct LightUniform {
    position: vec3<f32>,
    kind: u32,
    direction: vec3<f32>,
    cutoff: f32,    
    color: vec3<f32>,
    intensity: f32,
    transform_index: u32,
}

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(2) @binding(0)
var<storage, read> transforms: array<TransformUniform>;

@group(2) @binding(2)
var<storage, read> lights: array<LightUniform>;

@group(2) @binding(3)
var<storage, read> instance_indices: array<u32>;

@vertex
fn vs_main(
    mesh: VertexInput,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let light = lights[instance_id];
    let global_index = instance_indices[instance_id];
    let transform = transforms[global_index].matrix;

    let scale = 0.25;
    var out: VertexOutput;
    out.clip_position = camera.view_projection * transform * vec4<f32>(mesh.position * scale, 1.0);
    out.color = light.color;
    return out;
}

// Fragment shader
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
