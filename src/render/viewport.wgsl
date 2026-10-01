struct Uniforms {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    eye: vec4<f32>,
    wire_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: Uniforms;

struct MeshInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

fn mesh_vertex(input: MeshInput) -> VertexOutput {
    var output: VertexOutput;
    let world = scene.model * vec4(input.position, 1.0);
    output.clip_position = scene.view_projection * world;
    output.world_position = world.xyz;
    output.world_normal = (scene.model * vec4(input.normal, 0.0)).xyz;
    return output;
}

@vertex fn vs_mesh(input: MeshInput) -> VertexOutput {
    return mesh_vertex(input);
}

@vertex fn vs_edge(input: MeshInput) -> VertexOutput {
    var output = mesh_vertex(input);
    // Pull visible original polygon edges slightly toward the camera. Keeping
    // them separate from triangles avoids revealing triangulation diagonals.
    output.clip_position.z -= 0.00002 * output.clip_position.w;
    return output;
}

fn gamma_encode(linear: vec3<f32>) -> vec3<f32> {
    let color = max(linear, vec3(0.0));
    return select(
        1.055 * pow(color, vec3(1.0 / 2.4)) - vec3(0.055),
        12.92 * color,
        color <= vec3(0.0031308),
    );
}

fn surface_color(
    input: VertexOutput,
    front_facing: bool,
) -> vec3<f32> {
    let normal = normalize(select(-input.world_normal, input.world_normal, front_facing));
    let view = normalize(scene.eye.xyz - input.world_position);
    return gamma_encode(inspection_color(normal, view));
}

@fragment fn fs_surface(
    input: VertexOutput,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    return vec4(surface_color(input, front_facing), 1.0);
}

@fragment fn fs_xray_surface(
    input: VertexOutput,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    return vec4(surface_color(input, front_facing), 0.24);
}

@fragment fn fs_edge() -> @location(0) vec4<f32> {
    return vec4(gamma_encode(vec3(0.026, 0.036, 0.052)), 0.64);
}

@fragment fn fs_depth() -> @location(0) vec4<f32> {
    return vec4(0.0);
}

@fragment fn fs_wire() -> @location(0) vec4<f32> {
    return scene.wire_color;
}

@fragment fn fs_xray_edge() -> @location(0) vec4<f32> {
    return vec4(scene.wire_color.rgb, scene.wire_color.a * 0.36);
}

@vertex fn vs_grid(@builtin(vertex_index) index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2(-20.0, -20.0), vec2(20.0, -20.0), vec2(20.0, 20.0),
        vec2(-20.0, -20.0), vec2(20.0, 20.0), vec2(-20.0, 20.0),
    );
    var output: VertexOutput;
    output.world_position = vec3(corners[index].x, -1.05, corners[index].y);
    output.world_normal = vec3(0.0, 1.0, 0.0);
    output.clip_position = scene.view_projection * vec4(output.world_position, 1.0);
    return output;
}

fn grid_line(coordinates: vec2<f32>) -> f32 {
    let derivatives = max(fwidth(coordinates), vec2(0.0001));
    let distance = abs(fract(coordinates - vec2(0.5)) - vec2(0.5)) / derivatives;
    return 1.0 - min(min(distance.x, distance.y), 1.0);
}

@fragment fn fs_grid(input: VertexOutput) -> @location(0) vec4<f32> {
    let coordinates = input.world_position.xz;
    let minor = grid_line(coordinates * 4.0);
    let major = grid_line(coordinates);
    let fade = 1.0 - smoothstep(2.0, 11.0, length(coordinates));
    let derivatives = max(fwidth(coordinates), vec2(0.0001));
    let x_axis = 1.0 - min(abs(coordinates.y) / derivatives.y, 1.0);
    let z_axis = 1.0 - min(abs(coordinates.x) / derivatives.x, 1.0);
    var color = vec3(0.12, 0.16, 0.21);
    color = mix(color, vec3(0.25, 0.11, 0.12), x_axis * 0.7);
    color = mix(color, vec3(0.12, 0.19, 0.29), z_axis * 0.7);
    let alpha = max(max(minor * 0.17, major * 0.32), max(x_axis, z_axis) * 0.45) * fade;
    return vec4(gamma_encode(color), alpha);
}
