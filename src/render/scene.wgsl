// Metallic-roughness PBR shading with a built-in procedural studio environment:
// GGX-prefiltered specular mips, cosine diffuse convolution, and a BRDF LUT.
const PI: f32 = 3.14159265359;
struct Light {
    position_kind: vec4<f32>,
    direction_range: vec4<f32>,
    color_intensity: vec4<f32>,
    spot: vec4<f32>,
}
struct Globals {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    eye_exposure: vec4<f32>,
    settings: vec4<f32>, // meters/display-unit, light count, studio strength
    environment: vec4<f32>, // maximum specular IBL mip
    lights: array<Light, 32>,
}
struct UvTransform { x: vec4<f32>, y: vec4<f32> }
struct Material {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    pbr: vec4<f32>, // metallic, roughness, normal scale, occlusion strength
    flags: vec4<f32>, // alpha mode, cutoff, double sided, unlit
    uv: array<UvTransform, 5>,
}
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var base_texture: texture_2d<f32>;
@group(1) @binding(2) var base_sampler: sampler;
@group(1) @binding(3) var mr_texture: texture_2d<f32>;
@group(1) @binding(4) var mr_sampler: sampler;
@group(1) @binding(5) var normal_texture: texture_2d<f32>;
@group(1) @binding(6) var normal_sampler: sampler;
@group(1) @binding(7) var ao_texture: texture_2d<f32>;
@group(1) @binding(8) var ao_sampler: sampler;
@group(1) @binding(9) var emissive_texture: texture_2d<f32>;
@group(1) @binding(10) var emissive_sampler: sampler;
@group(2) @binding(0) var diffuse_environment: texture_cube<f32>;
@group(2) @binding(1) var specular_environment: texture_cube<f32>;
@group(2) @binding(2) var environment_brdf: texture_2d<f32>;
@group(2) @binding(3) var environment_sampler: sampler;
struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) color: vec4<f32>,
}
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) color: vec4<f32>,
}
@vertex fn vs_scene(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    let position = globals.model * vec4(input.position,1.0);
    out.clip = globals.view_projection * position;
    out.position = position.xyz * globals.settings.x;
    out.normal = (globals.model * vec4(input.normal,0.0)).xyz;
    out.tangent = vec4((globals.model * vec4(input.tangent.xyz,0.0)).xyz,input.tangent.w);
    out.uv0 = input.uv0;
    out.uv1 = input.uv1;
    out.color = input.color;
    return out;
}
fn uv_at(input: VertexOut, index: u32) -> vec2<f32> {
    let transform = material.uv[index];
    let source = select(input.uv0, input.uv1, transform.x.w > 0.5);
    return vec2(dot(transform.x.xy, source) + transform.x.z,
                dot(transform.y.xy, source) + transform.y.z);
}
fn safe_normalize(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let largest = max(max(abs(v.x), abs(v.y)), abs(v.z));
    let scaled = v / max(largest, 1e-20);
    return select(fallback, scaled * inverseSqrt(max(dot(scaled, scaled), 1e-16)), largest > 1e-12);
}
fn fresnel(f0: vec3<f32>, v_dot_h: f32) -> vec3<f32> {
    return f0 + (vec3(1.0) - f0) * pow(1.0 - clamp(v_dot_h, 0.0, 1.0), 5.0);
}
fn brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, base: vec3<f32>, metallic: f32, rough: f32) -> vec3<f32> {
    let h = safe_normalize(v + l, n);
    let nl = max(dot(n, l), 0.0);
    let nv = max(dot(n, v), 0.0001);
    let nh = max(dot(n, h), 0.0);
    let vh = max(dot(v, h), 0.0);
    let alpha = rough * rough;
    let a2 = alpha * alpha;
    let denominator = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(PI * denominator * denominator, 1e-7);
    // Height-correlated Smith GGX visibility, including the 1/(4*N.L*N.V).
    let gv = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    let gl = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    let visibility = 0.5 / max(gv + gl, 1e-6);
    let f = fresnel(mix(vec3(0.04), base, metallic), vh);
    let diffuse = (vec3(1.0) - f) * (1.0 - metallic) * base / PI;
    return (diffuse + distribution * visibility * f) * nl;
}
fn studio_ambient(n: vec3<f32>, v: vec3<f32>, base: vec3<f32>, metal: f32, rough: f32) -> vec3<f32> {
    let nv = max(dot(n, v), 0.0);
    let f0 = mix(vec3(0.04), base, metal);
    let f = fresnel(f0, nv);
    let irradiance = textureSampleLevel(diffuse_environment, environment_sampler, n, 0.0).rgb;
    let reflected = textureSampleLevel(specular_environment, environment_sampler, reflect(-v,n), rough * globals.environment.x).rgb;
    let integral = textureSampleLevel(environment_brdf, environment_sampler, vec2(nv,rough), 0.0).rg;
    let diffuse = (1.0-metal) * (vec3(1.0)-f) * base * irradiance;
    let specular = reflected * (f0 * integral.x + integral.y);
    return (diffuse + specular) * globals.settings.z;
}
fn tone_map(linear: vec3<f32>) -> vec3<f32> {
    let x = clamp(linear * exp2(globals.eye_exposure.w), vec3(0.0), vec3(1e12));
    // Compact ACES fit, bounded before quadratic arithmetic. Alpha blends in
    // linear LDR after this per-fragment tone map; full HDR compositing is future
    // work. The SRGB attachment encodes the resulting linear values.
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3(0.0), vec3(1.0));
}
fn coverage(alpha: f32) -> f32 {
    if material.flags.x < 0.5 { return 1.0; }
    if material.flags.x < 1.5 && alpha < material.flags.y { discard; }
    return select(1.0, alpha, material.flags.x > 1.5);
}
@fragment fn fs_unlit(input: VertexOut) -> @location(0) vec4<f32> {
    let base = textureSample(base_texture, base_sampler, uv_at(input, 0u)) * material.base_color * input.color;
    let alpha = coverage(base.a);
    return vec4(clamp(base.rgb * exp2(globals.eye_exposure.w), vec3(0.0), vec3(1.0)), alpha);
}
@fragment fn fs_solid(input: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Preserve source smooth normals after skinning/morphs and placement. This
    // double-sided, opaque inspection does not consult any material resources.
    let fallback = safe_normalize(cross(dpdx(input.position), dpdy(input.position)), vec3(0.0, 0.0, 1.0));
    let source_normal = safe_normalize(input.normal, fallback);
    let n = select(-source_normal, source_normal, front);
    let v = safe_normalize(globals.eye_exposure.xyz - input.position, n);
    return vec4(inspection_color(n, v), 1.0);
}
@fragment fn fs_scene(input: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let base = textureSample(base_texture, base_sampler, uv_at(input, 0u)) * material.base_color * input.color;
    let mr = textureSample(mr_texture, mr_sampler, uv_at(input, 1u));
    let normal_uv = uv_at(input, 2u);
    let sampled_normal = textureSample(normal_texture, normal_sampler, normal_uv).xyz * 2.0 - 1.0;
    let occlusion = textureSample(ao_texture, ao_sampler, uv_at(input, 3u)).r;
    let emissive = textureSample(emissive_texture, emissive_sampler, uv_at(input, 4u)).rgb * material.emissive.rgb;
    let dpdx_position = dpdx(input.position);
    let dpdy_position = dpdy(input.position);
    let duv_dx = dpdx(normal_uv);
    let duv_dy = dpdy(normal_uv);
    let alpha = coverage(base.a);
    if material.flags.w > 0.5 {
        return vec4(clamp(base.rgb * exp2(globals.eye_exposure.w), vec3(0.0), vec3(1.0)), alpha);
    }
    var n = safe_normalize(input.normal, safe_normalize(cross(dpdx_position, dpdy_position), vec3(0.0, 0.0, 1.0)));
    if material.uv[2].y.w > 0.5 {
        var tangent: vec3<f32>;
        var bitangent: vec3<f32>;
        let transform = material.uv[2];
        let identity_uv0 = transform.x.w < 0.5 && abs(transform.x.x - 1.0) < 1e-6 && abs(transform.y.y - 1.0) < 1e-6 && abs(transform.x.y) < 1e-6 && abs(transform.y.x) < 1e-6;
        if dot(input.tangent.xyz, input.tangent.xyz) > 1e-8 && identity_uv0 {
            tangent = safe_normalize(input.tangent.xyz - n * dot(n, input.tangent.xyz), vec3(1.0, 0.0, 0.0));
            bitangent = cross(n, tangent) * input.tangent.w;
        } else {
            // Derivatives honor the chosen UV set and texture transform when
            // authored tangents are absent or refer to a different UV basis.
            let determinant = duv_dx.x * duv_dy.y - duv_dx.y * duv_dy.x;
            let sign_uv = select(-1.0, 1.0, determinant >= 0.0);
            tangent = safe_normalize((dpdx_position * duv_dy.y - dpdy_position * duv_dx.y) * sign_uv, vec3(1.0, 0.0, 0.0));
            tangent = safe_normalize(tangent - n * dot(n, tangent), vec3(1.0, 0.0, 0.0));
            bitangent = safe_normalize(cross(n, tangent) * sign_uv, vec3(0.0, 1.0, 0.0));
        }
        let map = safe_normalize(vec3(sampled_normal.xy * material.pbr.z, sampled_normal.z), vec3(0.0, 0.0, 1.0));
        n = safe_normalize(tangent * map.x + bitangent * map.y + n * map.z, n);
    }
    // Reverse the complete mapped normal on a double-sided back face, including
    // its tangent components, rather than only flipping the geometric Z basis.
    if material.flags.z > 0.5 && !front { n = -n; }
    let v = safe_normalize(globals.eye_exposure.xyz - input.position, n);
    let metallic = clamp(material.pbr.x * mr.b, 0.0, 1.0);
    let roughness = clamp(material.pbr.y * mr.g, 0.045, 1.0);
    var radiance = vec3(0.0);
    for (var i = 0u; i < u32(globals.settings.y); i += 1u) {
        let light = globals.lights[i];
        var l = -light.direction_range.xyz;
        var attenuation = 1.0;
        if light.position_kind.w > 0.5 {
            let offset = light.position_kind.xyz - input.position;
            let distance_sq = max(dot(offset, offset), 1e-6);
            l = offset * inverseSqrt(distance_sq);
            attenuation = 1.0 / distance_sq;
            if light.direction_range.w > 0.0 {
                attenuation *= clamp(1.0 - pow(sqrt(distance_sq) / light.direction_range.w, 4.0), 0.0, 1.0);
            }
            if light.position_kind.w > 1.5 {
                let cosine = dot(-l, light.direction_range.xyz);
                let spot = clamp((cosine - light.spot.y) / max(light.spot.x - light.spot.y, 1e-6), 0.0, 1.0);
                attenuation *= spot * spot;
            }
        }
        radiance += brdf(n, v, l, base.rgb, metallic, roughness) * light.color_intensity.rgb * light.color_intensity.w * attenuation;
    }
    let ao = mix(1.0, occlusion, material.pbr.w);
    radiance += studio_ambient(n, v, base.rgb, metallic, roughness) * ao + emissive;
    return vec4(tone_map(radiance), alpha);
}

@fragment fn fs_wire() -> @location(0) vec4<f32> {
    return vec4(vec3(globals.settings.w), 1.0);
}
