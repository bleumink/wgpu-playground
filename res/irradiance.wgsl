// irradiance_fixed_full.wgsl
const PI : f32 = 3.141592653589793;

// Bindings:
// 0 = src cubemap (texture_cube<f32>)
// 1 = sampler
// 2 = dst irradiance storage cube
@group(0) @binding(0) var src_cubemap : texture_cube<f32>;
@group(0) @binding(1) var src_sampler : sampler;
@group(0) @binding(2) var dst_irradiance : texture_storage_2d_array<rgba16float, write>;

// Sampling resolution
const N_THETA : u32 = 32u;
const N_PHI   : u32 = N_THETA * 2u;

// Standard WGPU cube mapping
fn face_uv_to_dir(face : u32, uv : vec2<f32>) -> vec3<f32> {
    let sc = uv.x;
    let tc = uv.y;

    switch (face) {
        case 0u: { return normalize(vec3<f32>( 1.0,   -tc,  -sc)); } // +X
        case 1u: { return normalize(vec3<f32>(-1.0,   -tc,   sc)); } // -X
        case 2u: { return normalize(vec3<f32>( sc,    1.0,   tc)); } // +Y
        case 3u: { return normalize(vec3<f32>( sc,   -1.0,  -tc)); } // -Y
        case 4u: { return normalize(vec3<f32>( sc,   -tc,   1.0)); } // +Z
        default: { return normalize(vec3<f32>(-sc,  -tc,  -1.0));  } // -Z
    }
}

@compute
@workgroup_size(8, 8, 1)
fn irradiance_convolution(@builtin(global_invocation_id) gid : vec3<u32>) {

    // Cube storage textures give per-face 2D dimensions:
    let tex_size : vec2<u32> = textureDimensions(dst_irradiance);
    let width  : u32 = tex_size.x;
    let height : u32 = tex_size.y;

    let face : u32 = gid.z;
    if (gid.x >= width || gid.y >= height || face >= 6u) {
        return;
    }

    // Pixel -> uv [-1,1]
    let px  = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);
    let uv0 = (px / vec2<f32>(f32(width), f32(height))) * 2.0 - vec2<f32>(1.0, 1.0);

    // Flip Y for cubemap
    let uv = vec2<f32>(uv0.x, -uv0.y);

    let normal = face_uv_to_dir(face, uv);

    // Build tangent frame
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(normal.y) > 0.999) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }

    let tangent   = normalize(cross(up, normal));
    let bitangent = normalize(cross(normal, tangent));

    // Hemisphere integration
    let d_theta = (0.5 * PI) / f32(N_THETA);
    let d_phi   = (2.0 * PI) / f32(N_PHI);

    var accum = vec3<f32>(0.0, 0.0, 0.0);

    for (var ti : u32 = 0u; ti < N_THETA; ti = ti + 1u) {
        let theta     = (f32(ti) + 0.5) * d_theta;
        let sin_theta = sin(theta);
        let cos_theta = cos(theta);

        for (var pi : u32 = 0u; pi < N_PHI; pi = pi + 1u) {
            let phi = (f32(pi) + 0.5) * d_phi;

            let dir = normalize(
                tangent   * (sin_theta * cos(phi)) +
                bitangent * (sin_theta * sin(phi)) +
                normal    * cos_theta
            );

            let sample_color = textureSampleLevel(src_cubemap, src_sampler, dir, 0.0).rgb;
            let weight = cos_theta * sin_theta * d_theta * d_phi;

            accum = accum + sample_color * weight;
        }
    }

    // (Optional but correct) Normalize Lambert hemisphere integral
    let result = accum * (1.0 / PI);

    textureStore(
        dst_irradiance,
        vec2<i32>(i32(gid.x), i32(gid.y)),
        i32(face),
        vec4<f32>(result, 1.0)
    );
}
