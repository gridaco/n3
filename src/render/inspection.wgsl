// Shared fixed editor material, in linear color. Viewport shading encodes this
// explicitly; the imported pass writes through an sRGB attachment instead.
// Authored material values, textures, lights and exposure never enter this path.
fn inspection_color(normal: vec3<f32>, view: vec3<f32>) -> vec3<f32> {
    let key = normalize(vec3(-0.6, 0.9, 0.8));
    let fill = normalize(vec3(0.8, 0.35, -0.5));
    let top = normalize(vec3(0.1, 1.0, -0.3));
    let diffuse = 0.34
        + 0.57 * max(dot(normal, key), 0.0)
        + 0.23 * max(dot(normal, fill), 0.0)
        + 0.12 * max(dot(normal, top), 0.0);
    let half_vector = normalize(key + view);
    let highlight = pow(max(dot(normal, half_vector), 0.0), 48.0) * 0.15;
    let rim = pow(1.0 - max(dot(normal, view), 0.0), 3.0) * 0.035;
    let base_color = vec3(0.32, 0.43, 0.54);
    return base_color * diffuse + vec3(highlight) + vec3(0.65, 0.78, 0.95) * rim;
}
