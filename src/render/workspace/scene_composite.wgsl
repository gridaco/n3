// The production scene is gamma encoded in Rgba8Unorm. Match egui-wgpu 0.36.2
// default linear sampling and gamma-space dithering, and undo gamma before an
// sRGB attachment applies its automatic conversion. The scene is always opaque.
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> rect: vec4<f32>;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
    );
    var out: Vertex;
    out.uv = corners[index];
    out.position = vec4(mix(rect.xy, rect.zw, out.uv), 0.0, 1.0);
    return out;
}
fn sample_gamma(in: Vertex) -> vec4<f32> {
    let color = textureSample(scene, scene_sampler, in.uv);
    let f = 0.06711056 * in.position.x + 0.00583715 * in.position.y;
    let noise = (fract(52.9829189 * fract(f)) - 0.5) * 0.95;
    return vec4(color.rgb + noise / 255.0, color.a);
}
@fragment
fn fs_gamma(in: Vertex) -> @location(0) vec4<f32> {
    return sample_gamma(in);
}
@fragment
fn fs_srgb(in: Vertex) -> @location(0) vec4<f32> {
    let color = sample_gamma(in);
    let low = color.rgb / vec3(12.92);
    let high = pow((color.rgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
    return vec4(select(high, low, color.rgb < vec3(0.04045)), color.a);
}
