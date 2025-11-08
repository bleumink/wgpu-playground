// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,    
    @location(3) uv1: vec2<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) uv3: vec2<f32>,
    @location(6) uv4: vec2<f32>,
    @location(7) uv5: vec2<f32>,
    @location(8) uv6: vec2<f32>,
}

struct InstanceInput {
    @location(9) transform_index: u32, 
    @location(10) normal_index: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,    
    @location(2) tangent: vec4<f32>,
    @location(3) tex_coords: vec2<f32>,
    @location(4) view_position: vec3<f32>,
}

struct CameraUniform {
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
}

struct TransformUniform {
    matrix: mat4x4<f32>,
}

struct NormalUniform {
    matrix: mat4x4<f32>,
}

struct LightUniform {
    color: vec3<f32>,
    cutoff: f32,    
    intensity: f32,  
    kind: u32,  
    padding: vec2<u32>,
}

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@group(2) @binding(0)
var<storage, read> transforms: array<TransformUniform>;

@group(2) @binding(1)
var<storage, read> normals: array<NormalUniform>;

@group(2) @binding(2)
var<storage, read> lights: array<LightUniform>;

@group(2) @binding(3)
var<storage, read> light_transform_index: array<u32>;


@vertex
fn vs_main(
    mesh: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model = transforms[instance.transform_index].matrix;
    let normal_matrix = mat4_to_mat3(normals[instance.normal_index].matrix);
    
    let world_position = model * vec4<f32>(mesh.position, 1.0);    
    let world_normal =  normal_matrix * mesh.normal;

    var out: VertexOutput;
    out.world_position = world_position.xyz;
    out.normal = world_normal;
    out.tangent = mesh.tangent;
    out.tex_coords = mesh.uv1;
    out.view_position = camera.view_position.xyz;
    out.clip_position = camera.view_projection * world_position;
    return out;
}

// Fragment shader
struct MaterialUniform {
    base_color_factor: vec4<f32>,
    emissive_factor: vec3<f32>,
    metallic_factor: f32,
    roughness_factor: f32,
    occlusion_strength: f32,
    normal_scale: f32,
    alpha_cutoff: f32,
    alpha_mode: u32,
    double_sided: u32,
}

struct LightModel {
    position: vec3<f32>,
    direction: vec3<f32>,
}

@group(0) @binding(0) var<uniform> materialUniform: MaterialUniform;
@group(0) @binding(1) var baseColorTexture: texture_2d<f32>;
@group(0) @binding(2) var baseColorSampler: sampler;
@group(0) @binding(3) var metallicRoughnessTexture: texture_2d<f32>;
@group(0) @binding(4) var metallicRoughnessSampler: sampler;
@group(0) @binding(5) var normalTexture: texture_2d<f32>;
@group(0) @binding(6) var normalSampler: sampler;
@group(0) @binding(7) var occlusionTexture: texture_2d<f32>;
@group(0) @binding(8) var occlusionSampler: sampler;
@group(0) @binding(9) var emissiveTexture: texture_2d<f32>;
@group(0) @binding(10) var emissiveSampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {        
    let n = normalize(in.normal);
    let v = normalize(in.view_position - in.world_position);
    
    let base_color_sample = textureSample(baseColorTexture, baseColorSampler, in.tex_coords).rgb;
    let albedo = pow(base_color_sample, vec3<f32>(2.2));
    
    let mr_sample = textureSample(metallicRoughnessTexture, metallicRoughnessSampler, in.tex_coords).rgb;
    let metallic = mr_sample.b;
    let roughness = clamp(mr_sample.g, 0.04, 1.0);
    
    let occlusion = 1.0;
    // let occlusion = textureSample(occlusionTexture, occlusionSampler, in.tex_coords).r;
    let PI = 3.14159265;

    var f0 = mix(vec3<f32>(0.04), albedo, metallic);
    var lo = vec3<f32>(0.0);
    
    for (var i = 0u; i < arrayLength(&lights); i++) {
        let transform_index = light_transform_index[i];
        let model = from_transform(transforms[transform_index].matrix);        
        let light = lights[i];
                
        var l = vec3<f32>(0.0);
        var attenuation = 1.0;

        switch light.kind {
            case 0u: { // directional
                l = normalize(-model.direction);
            }
            case 1u: { // point
                let to_light = model.position - in.world_position; 
                let distance = length(to_light);
                l = normalize(to_light);                
                attenuation = 1.0 / max(distance * distance, 0.0001);
            }
            case 2u: { // spot
                let to_light = model.position - in.world_position;
                let distance = length(to_light);                
                l = normalize(to_light);
                attenuation = 1.0 / max(distance * distance, 0.0001);                
            }
            default: {}
        }

        let h = normalize(v + l);

        let n_dot_v = max(dot(n, v), 0.0);
        let n_dot_l = max(dot(n, l), 0.0);

        if (n_dot_l <= 0.0) {
            continue;
        }

        let ndf = distribution_ggx(n, h, roughness);
        let g = geometry_smith(n, v, l, roughness);
        let f = fresnel_schlick(max(dot(h, v), 0.0), f0);

        let denominator = max(4.0 * n_dot_v * n_dot_l, 0.0001);
        let specular = (ndf * g * f) / denominator;

        let ks = f;
        let kd = (vec3<f32>(1.0) - ks) * (1.0 - metallic);

        let diffuse = kd * albedo / PI;
        let radiance = light.color * light.intensity * attenuation;

        lo += (diffuse + specular) * radiance * n_dot_l;
    }

    let ambient = vec3<f32>(0.001) * albedo * occlusion;
    var color = lo + ambient;

    // Tone map and gamma correct
    let mapped = color / (color + vec3<f32>(1.0));
    let out = pow(mapped, vec3<f32>(1.0 / 2.2));
    // return vec4<f32>(n * 0.5 + 0.5, 1.0);
    return vec4<f32>(out, 1.0);    
}

fn mat4_to_mat3(matrix: mat4x4<f32>) -> mat3x3<f32> {
    return mat3x3<f32>(
        matrix[0].xyz,
        matrix[1].xyz,
        matrix[2].xyz,
    );
}

fn from_transform(matrix: mat4x4<f32>) -> LightModel {
    var model: LightModel;
    model.position = matrix[3].xyz;
    model.direction = normalize(-matrix[2].xyz);

    return model;
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

fn distribution_ggx(n: vec3<f32>, h: vec3<f32>, roughness: f32) -> f32 {
    let a      = roughness * roughness;
    let a2     = a * a;
    let n_dot_h  = max(dot(n, h), 0.0);
    let n_dot_h2 = n_dot_h * n_dot_h;

    let numerator   = a2;
    let denominator = (n_dot_h2 * (a2 - 1.0) + 1.0);
    return numerator / (3.14159265 * denominator * denominator);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = (roughness + 1.0);
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

fn geometry_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32) -> f32 {
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_l = max(dot(n, l), 0.0);
    let ggx1 = geometry_schlick_ggx(n_dot_v, roughness);
    let ggx2 = geometry_schlick_ggx(n_dot_l, roughness);
    return ggx1 * ggx2;
}

fn getNormalFromMap(normalTexSample: vec3<f32>, normal: vec3<f32>, tangent: vec3<f32>, bitangent: vec3<f32>) -> vec3<f32> {
    let n = normalize(normalTexSample * 2.0 - 1.0);
    let t = normalize(tangent);
    let b = normalize(bitangent);
    let tbn = mat3x3<f32>(t, b, normal);
    return normalize(tbn * n);
}