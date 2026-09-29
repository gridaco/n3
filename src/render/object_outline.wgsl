struct Scene {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    eye: vec4<f32>,
};

struct Outline {
    selected: vec4<f32>,
    hovered: vec4<f32>,
    widths: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(1) @binding(0) var object_mask: texture_2d<u32>;
@group(1) @binding(1) var<uniform> outline: Outline;

struct MaskVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) state: u32,
};

@vertex fn vs_mask(
    @location(0) position: vec3<f32>,
    @builtin(instance_index) state: u32,
) -> MaskVertex {
    var vertex: MaskVertex;
    vertex.position = scene.view_projection * (scene.model * vec4(position, 1.0));
    vertex.state = state;
    return vertex;
}

@fragment fn fs_mask(vertex: MaskVertex) -> @location(0) u32 {
    return vertex.state;
}

@vertex fn vs_outline(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(corners[index], 0.0, 1.0);
}

fn mask_at(pixel: vec2<i32>) -> u32 {
    let size = vec2<i32>(textureDimensions(object_mask));
    if any(pixel < vec2(0)) || any(pixel >= size) {
        return 0u;
    }
    return textureLoad(object_mask, pixel, 0).r;
}

@fragment fn fs_outline(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let state = mask_at(pixel);
    // An inner boundary never paints through or over an occluding object.
    if state == 0u {
        discard;
    }
    let selected = state == 1u;
    let width = select(outline.widths.y, outline.widths.x, selected);
    let directions = array<vec2<i32>, 8>(
        vec2(1, 0), vec2(-1, 0), vec2(0, 1), vec2(0, -1),
        vec2(1, 1), vec2(1, -1), vec2(-1, 1), vec2(-1, -1),
    );
    var nearest = width + 1.0;
    for (var step = 1; step <= i32(ceil(width)); step += 1) {
        for (var index = 0; index < 8; index += 1) {
            let offset = directions[index] * step;
            if mask_at(pixel + offset) != state {
                nearest = min(nearest, length(vec2<f32>(offset)));
            }
        }
    }
    let coverage = clamp(width + 1.0 - nearest, 0.0, 1.0);
    if coverage == 0.0 {
        discard;
    }
    let color = select(outline.hovered, outline.selected, selected);
    return vec4(color.rgb, color.a * coverage);
}
