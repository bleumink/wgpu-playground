// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) intensity: f32,    
}

struct InstanceInput {
    @location(3) transform_index: u32, 
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

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(2) @binding(0)
var<storage, read> transforms: array<TransformUniform>;

@vertex
fn vs_main(
    points: VertexInput,    
    instance: InstanceInput,
) -> VertexOutput {
    let transform = transforms[instance.transform_index].matrix;

    var out: VertexOutput;
    out.clip_position = camera.view_projection * transform * vec4<f32>(points.position, 1.0);
    out.color = points.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);    
}