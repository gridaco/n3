struct Scene {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    eye: vec4<f32>,
};
struct Style {
    selected: vec4<f32>,
    unselected: vec4<f32>,
    // viewport pixels, visible line width in pixels, antialias skirt in pixels
    metrics: vec4<f32>,
};
@group(0) @binding(0) var<uniform> scene: Scene;
@group(1) @binding(0) var<uniform> style: Style;

struct MeshInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn vs_face(input: MeshInput) -> @builtin(position) vec4<f32> {
    let world = scene.model * vec4(input.position, 1.0);
    return scene.view_projection * world;
}
@fragment fn fs_face() -> @location(0) vec4<f32> {
    // These palette values are already display encoded, matching the scene's
    // gamma-encoded non-sRGB target. Alpha keeps the original lighting visible.
    return style.selected;
}

@fragment fn fs_hidden_face() -> @location(0) vec4<f32> {
    return vec4(style.selected.rgb, style.selected.a * 0.45);
}

struct EdgeInput {
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
    @location(2) selected: vec2<f32>,
};
struct EdgeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(linear) selected: f32,
    @location(1) @interpolate(linear) across: f32,
};
fn hidden_edge() -> EdgeOutput {
    var output: EdgeOutput;
    output.position = vec4(2.0, 2.0, 2.0, 1.0);
    output.selected = 0.0;
    output.across = 0.0;
    return output;
}
@vertex fn vs_edge(input: EdgeInput, @builtin(vertex_index) index: u32) -> EdgeOutput {
    var a = scene.view_projection * (scene.model * vec4(input.start, 1.0));
    var b = scene.view_projection * (scene.model * vec4(input.end, 1.0));
    var selected = input.selected;
    // Clip in homogeneous space before dividing by w. Crossing the near plane
    // must create a shorter segment, never a screen-sized flipped ribbon.
    let minimum_w = 0.0000001;
    if a.w <= minimum_w && b.w <= minimum_w { return hidden_edge(); }
    if a.w < minimum_w {
        let t = (minimum_w - a.w) / (b.w - a.w);
        a = mix(a, b, t);
        selected.x = mix(selected.x, selected.y, t);
    }
    if b.w < minimum_w {
        let t = (minimum_w - b.w) / (a.w - b.w);
        b = mix(b, a, t);
        selected.y = mix(selected.y, selected.x, t);
    }
    if a.z < 0.0 && b.z < 0.0 { return hidden_edge(); }
    if a.z < 0.0 {
        let t = -a.z / (b.z - a.z);
        a = mix(a, b, t);
        selected.x = mix(selected.x, selected.y, t);
    }
    if b.z < 0.0 {
        let t = -b.z / (a.z - b.z);
        b = mix(b, a, t);
        selected.y = mix(selected.y, selected.x, t);
    }
    let direction = (b.xy / b.w - a.xy / a.w) * style.metrics.xy;
    let length_squared = dot(direction, direction);
    if length_squared < 0.000001 { return hidden_edge(); }
    let perpendicular = vec2(-direction.y, direction.x) * inverseSqrt(length_squared);
    let corners = array<vec2<f32>, 6>(
        vec2(0.0,-1.0), vec2(1.0,-1.0), vec2(1.0,1.0),
        vec2(0.0,-1.0), vec2(1.0,1.0), vec2(0.0,1.0),
    );
    let corner = corners[index];
    let half_width = style.metrics.z * 0.5 + style.metrics.w;
    var clip = mix(a, b, corner.x);
    clip.x += perpendicular.x * corner.y * half_width * 2.0 / style.metrics.x * clip.w;
    clip.y += perpendicular.y * corner.y * half_width * 2.0 / style.metrics.y * clip.w;
    var output: EdgeOutput;
    output.position = clip;
    output.selected = mix(selected.x, selected.y, corner.x);
    output.across = corner.y * half_width;
    return output;
}
fn edge_color(input: EdgeOutput) -> vec4<f32> {
    let half_width = style.metrics.z * 0.5;
    let alpha = 1.0 - smoothstep(max(0.0, half_width - style.metrics.w),
        half_width + style.metrics.w, abs(input.across));
    return vec4(mix(style.unselected.rgb, style.selected.rgb, input.selected), alpha);
}

@fragment fn fs_edge(input: EdgeOutput) -> @location(0) vec4<f32> {
    return edge_color(input);
}

@fragment fn fs_hidden_edge(input: EdgeOutput) -> @location(0) vec4<f32> {
    let color = edge_color(input);
    return vec4(color.rgb, color.a * 0.45);
}
