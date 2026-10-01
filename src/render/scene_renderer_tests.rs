//! Read pixels from the real GPU path. These checks exercise material behavior,
//! not screenshot baselines or shader compilation alone.
use super::*;
use crate::{
    camera::View,
    doc_capture::Capture,
    scene::{AlphaMode, Material, SceneVertex, TextureInfo, TextureTransform},
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

const SIZE: u32 = 64;
fn material() -> Material {
    Material {
        name: "test".into(),
        base_color: [1.0; 4],
        metallic: 0.0,
        roughness: 0.8,
        emissive: [0.0; 3],
        base_color_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        normal_scale: 1.0,
        occlusion_texture: None,
        occlusion_strength: 1.0,
        emissive_texture: None,
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        unlit: true,
    }
}
fn frame(material: usize, z: f64) -> EvaluatedScene {
    EvaluatedScene {
        draws: vec![scene::EvaluatedDraw {
            node: 0,
            mesh: 0,
            primitive: 0,
            material,
            topology: Topology::Triangles,
            vertices: [
                [-100.0, -100.0, z],
                [100.0, -100.0, z],
                [100.0, 100.0, z],
                [-100.0, 100.0, z],
            ]
            .map(|position| SceneVertex {
                position,
                normal: [0.0, 0.0, 1.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                uv0: [0.25, 0.5],
                uv1: [0.75, 0.5],
                color: [1.0; 4],
            })
            .to_vec(),
            indices: Arc::from([0, 1, 2, 0, 2, 3]),
            mirrored: false,
        }],
        bounds: Some(Bounds {
            min: DVec3::splat(-100.0),
            max: DVec3::splat(100.0),
        }),
        ..Default::default()
    }
}
fn configure(
    renderer: &mut PbrSceneRenderer,
    capture: &Capture,
    materials: &[Material],
    images: &[scene::Image],
    textures: &[scene::Texture],
) {
    renderer.materials = textures::materials(
        &capture.device,
        &capture.queue,
        &renderer.material_layout,
        materials,
        images,
        textures,
    )
    .unwrap();
}
fn texture(sampler: scene::Sampler) -> scene::Texture {
    scene::Texture { image: 0, sampler }
}
fn sampler(wrap: scene::Wrap) -> scene::Sampler {
    scene::Sampler {
        wrap_s: wrap,
        wrap_t: wrap,
        mag: scene::Filter::Nearest,
        min: scene::Filter::Nearest,
        mipmap: None,
    }
}
fn info(uv: u32) -> TextureInfo {
    TextureInfo {
        texture: 0,
        tex_coord: uv,
        transform: TextureTransform::default(),
    }
}
fn render(renderer: &mut PbrSceneRenderer, capture: &Capture, frame: &EvaluatedScene) -> Vec<u8> {
    renderer
        .set_frame(&capture.device, &capture.queue, frame)
        .unwrap();
    let mut camera = Camera::default();
    camera.set_view(View::Front);
    render_camera(renderer, capture, &camera, None)
}
fn render_camera(
    renderer: &PbrSceneRenderer,
    capture: &Capture,
    camera: &Camera,
    imported: Option<usize>,
) -> Vec<u8> {
    let mut encoder = capture.device.create_command_encoder(&Default::default());
    renderer
        .render(
            &capture.queue,
            &mut encoder,
            camera,
            egui::Color32::BLACK,
            0.0,
            imported,
        )
        .unwrap();
    let stride = (renderer.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = capture.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("n3 PBR test readback"),
        size: (stride * renderer.height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        renderer.color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(renderer.height),
            },
        },
        renderer.color.size(),
    );
    let submission = capture.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    capture
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    bytes
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..renderer.width as usize * 4].iter().copied())
        .collect()
}
fn center(pixels: &[u8]) -> [u8; 4] {
    pixels[((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
fn near(actual: u8, expected: u8) -> bool {
    actual.abs_diff(expected) <= 2
}

#[test]
fn pbr_gpu_decodes_color_textures_once_and_multiplies_linear_vertex_color() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.base_color_texture = Some(info(0));
    let image = scene::Image {
        name: "gray".into(),
        width: 1,
        height: 1,
        rgba8: vec![128, 128, 128, 255],
    };
    configure(
        &mut renderer,
        &capture,
        &[mat],
        &[image],
        &[texture(sampler(scene::Wrap::Repeat))],
    );
    let mut geometry = frame(0, 0.0);
    assert!(near(
        center(&render(&mut renderer, &capture, &geometry))[0],
        128
    ));
    for vertex in &mut geometry.draws[0].vertices {
        vertex.color = [0.5, 0.5, 0.5, 1.0];
    }
    let pixel = center(&render(&mut renderer, &capture, &geometry));
    assert!(
        near(pixel[0], 92) && pixel[0] == pixel[1] && pixel[1] == pixel[2],
        "linear multiplication: {pixel:?}"
    );
}

#[test]
fn pbr_gpu_distinguishes_opaque_mask_and_sorted_linear_alpha_blending() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.base_color = [1.0, 0.0, 0.0, 0.0];
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    assert_eq!(
        center(&render(&mut renderer, &capture, &frame(0, 0.0))),
        [255, 0, 0, 255]
    );
    mat.alpha_mode = AlphaMode::Mask;
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    assert_eq!(
        center(&render(&mut renderer, &capture, &frame(0, 0.0))),
        [0, 0, 0, 255]
    );
    mat.alpha_mode = AlphaMode::Blend;
    mat.base_color[3] = 0.5;
    let mut back = mat.clone();
    back.base_color = [0.0, 0.0, 1.0, 0.5];
    configure(&mut renderer, &capture, &[mat, back], &[], &[]);
    let mut geometry = frame(0, 10.0); // Supply near first to require explicit sorting.
    let mut far = frame(1, -10.0).draws.remove(0);
    far.node = 1;
    geometry.draws.push(far);
    let pixel = center(&render(&mut renderer, &capture, &geometry));
    assert!(
        near(pixel[0], 188) && pixel[1] == 0 && near(pixel[2], 137),
        "linear sorted blend: {pixel:?}"
    );
}

#[test]
fn pbr_gpu_reflections_and_double_sided_materials_keep_front_face_semantics() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.base_color = [0.0, 1.0, 0.0, 1.0];
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    let mut geometry = frame(0, 0.0);
    for vertex in &mut geometry.draws[0].vertices {
        vertex.position[0] *= -1.0;
    }
    assert_eq!(
        center(&render(&mut renderer, &capture, &geometry)),
        [0, 0, 0, 255]
    );
    geometry.draws[0].mirrored = true;
    assert_eq!(
        center(&render(&mut renderer, &capture, &geometry)),
        [0, 255, 0, 255]
    );
    geometry.draws[0].mirrored = false;
    mat.double_sided = true;
    configure(&mut renderer, &capture, &[mat], &[], &[]);
    assert_eq!(
        center(&render(&mut renderer, &capture, &geometry)),
        [0, 255, 0, 255]
    );
}

#[test]
fn pbr_gpu_uv_sets_transforms_and_sampler_wraps_select_the_expected_texel() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let image = scene::Image {
        name: "red green".into(),
        width: 2,
        height: 1,
        rgba8: vec![255, 0, 0, 255, 0, 255, 0, 255],
    };
    let mut mat = material();
    mat.base_color_texture = Some(info(0));
    let geometry = frame(0, 0.0);
    let mut draw = |mat: &Material, wrap| {
        configure(
            &mut renderer,
            &capture,
            std::slice::from_ref(mat),
            std::slice::from_ref(&image),
            &[texture(sampler(wrap))],
        );
        center(&render(&mut renderer, &capture, &geometry))
    };
    assert_eq!(draw(&mat, scene::Wrap::Repeat), [255, 0, 0, 255]);
    mat.base_color_texture = Some(info(1));
    assert_eq!(draw(&mat, scene::Wrap::Repeat), [0, 255, 0, 255]);
    mat.base_color_texture = Some(TextureInfo {
        transform: TextureTransform {
            offset: [0.5, 0.0],
            ..Default::default()
        },
        ..info(0)
    });
    assert_eq!(draw(&mat, scene::Wrap::Repeat), [0, 255, 0, 255]);
    mat.base_color_texture.as_mut().unwrap().transform.offset[0] = 1.0;
    assert_eq!(draw(&mat, scene::Wrap::Repeat), [255, 0, 0, 255]);
    assert_eq!(draw(&mat, scene::Wrap::Clamp), [0, 255, 0, 255]);
    assert_eq!(draw(&mat, scene::Wrap::Mirror), [0, 255, 0, 255]);
}

#[test]
fn pbr_gpu_material_channels_normal_maps_and_punctual_lights_affect_lit_surfaces() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.unlit = false;
    mat.base_color = [0.8, 0.15, 0.04, 1.0];
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    let geometry = frame(0, 0.0);
    let rough_dielectric = render(&mut renderer, &capture, &geometry);
    mat.metallic = 1.0;
    mat.roughness = 0.15;
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    let polished_metal = render(&mut renderer, &capture, &geometry);
    assert_ne!(
        rough_dielectric, polished_metal,
        "metal and roughness change reflected lighting"
    );
    mat.metallic = 0.0;
    mat.roughness = 0.8;
    mat.normal_texture = Some(info(0));
    let tilted_normal = scene::Image {
        name: "tilt".into(),
        width: 1,
        height: 1,
        rgba8: vec![255, 128, 128, 255],
    };
    configure(
        &mut renderer,
        &capture,
        &[mat.clone()],
        &[tilted_normal],
        &[texture(sampler(scene::Wrap::Repeat))],
    );
    assert_ne!(
        render(&mut renderer, &capture, &geometry),
        rough_dielectric,
        "normal texture changes the BRDF normal"
    );

    mat.normal_texture = None;
    configure(&mut renderer, &capture, &[mat], &[], &[]);
    renderer.lights = vec![scene::Light {
        name: "point".into(),
        kind: scene::LightKind::Point,
        color: [1.0; 3],
        intensity: 5.0,
        range_cm: None,
    }];
    let mut lit = geometry.clone();
    lit.lights.push(scene::EvaluatedLight {
        node: 1,
        light: 0,
        position_cm: DVec3::new(0.0, 0.0, 100.0),
        direction: DVec3::NEG_Z,
    });
    let near = center(&render(&mut renderer, &capture, &lit));
    lit.lights[0].position_cm.z = 200.0;
    let far = center(&render(&mut renderer, &capture, &lit));
    assert!(
        near[0] > far[0] && near[1] > far[1],
        "light attenuation uses physical meters: near={near:?}, far={far:?}"
    );
    renderer.lights[0].kind = scene::LightKind::Spot {
        inner: 0.1,
        outer: 0.3,
    };
    lit.lights[0].direction = DVec3::X;
    let outside = center(&render(&mut renderer, &capture, &lit));
    lit.lights[0].direction = DVec3::NEG_Z;
    let inside = center(&render(&mut renderer, &capture, &lit));
    assert!(
        inside[0] > outside[0],
        "spot cones attenuate outside their direction"
    );
}

#[test]
fn pbr_gpu_occlusion_does_not_suppress_emission_and_mr_channels_are_linear() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.unlit = false;
    mat.base_color = [0.6, 0.6, 0.6, 1.0];
    mat.metallic = 1.0;
    mat.roughness = 1.0;
    mat.metallic_roughness_texture = Some(info(0));
    let image = scene::Image {
        name: "packed MR".into(),
        width: 1,
        height: 1,
        rgba8: vec![255, 128, 64, 255],
    };
    configure(
        &mut renderer,
        &capture,
        &[mat.clone()],
        &[image],
        &[texture(sampler(scene::Wrap::Repeat))],
    );
    let geometry = frame(0, 0.0);
    let textured = render(&mut renderer, &capture, &geometry);
    mat.metallic_roughness_texture = None;
    mat.metallic = 64.0 / 255.0;
    mat.roughness = 128.0 / 255.0;
    configure(&mut renderer, &capture, &[mat.clone()], &[], &[]);
    assert_eq!(
        textured,
        render(&mut renderer, &capture, &geometry),
        "G roughness and B metallic are linear data"
    );
    mat.base_color = [0.0, 0.0, 0.0, 1.0];
    mat.emissive = [0.0, 1.0, 0.0];
    mat.occlusion_texture = Some(info(0));
    let black = scene::Image {
        name: "occluded".into(),
        width: 1,
        height: 1,
        rgba8: vec![0, 0, 0, 255],
    };
    configure(
        &mut renderer,
        &capture,
        &[mat],
        &[black],
        &[texture(sampler(scene::Wrap::Repeat))],
    );
    let emitted = center(&render(&mut renderer, &capture, &geometry));
    assert!(
        emitted[1] > 220 && emitted[1] > emitted[0] + 100,
        "AO cannot darken emission: {emitted:?}"
    );
}

#[test]
fn pbr_gpu_keeps_animation_framing_stable_and_uses_evaluated_imported_cameras() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    let mut mat = material();
    mat.base_color = [1.0, 0.0, 0.0, 1.0];
    configure(&mut renderer, &capture, &[mat], &[], &[]);
    let mut geometry = frame(0, 0.0);
    render(&mut renderer, &capture, &geometry);
    let transform = renderer.transform;
    geometry.bounds = Some(Bounds {
        min: DVec3::splat(1000.0),
        max: DVec3::splat(2000.0),
    });
    renderer
        .set_frame(&capture.device, &capture.queue, &geometry)
        .unwrap();
    assert_eq!(
        renderer.transform, transform,
        "animated bounds cannot recenter the view"
    );
    renderer.camera_definitions = vec![scene::Camera {
        name: "authored".into(),
        projection: scene::Projection::Perspective {
            yfov: 0.8,
            aspect: None,
            near_cm: 1.0,
            far_cm: None,
        },
    }];
    geometry.cameras.push(scene::EvaluatedCamera {
        node: 1,
        camera: 0,
        world: DMat4::from_translation(DVec3::new(0.0, 0.0, 300.0)),
    });
    renderer
        .set_frame(&capture.device, &capture.queue, &geometry)
        .unwrap();
    assert_eq!(
        center(&render_camera(
            &renderer,
            &capture,
            &Camera::default(),
            Some(0)
        )),
        [255, 0, 0, 255]
    );
    geometry.cameras[0].world = DMat4::from_translation(DVec3::new(1000.0, 0.0, 300.0));
    renderer
        .set_frame(&capture.device, &capture.queue, &geometry)
        .unwrap();
    assert_eq!(
        center(&render_camera(
            &renderer,
            &capture,
            &Camera::default(),
            Some(0)
        )),
        [0, 0, 0, 255]
    );
}

#[test]
fn pbr_gpu_wireframe_removes_fill_and_restores_the_same_materials() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    configure(&mut renderer, &capture, &[material()], &[], &[]);
    let geometry = frame(0, 0.0);
    let solid = render(&mut renderer, &capture, &geometry);
    renderer.set_wireframe(true);
    let wire = render(&mut renderer, &capture, &geometry);
    let colored = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 0)
            .count()
    };
    assert!(colored(&wire) > 0 && colored(&wire) < colored(&solid) / 2);
    renderer.set_wireframe(false);
    assert_eq!(render(&mut renderer, &capture, &geometry), solid);
}

#[test]
fn pbr_gpu_authored_camera_frames_preserve_square_geometry_in_wide_targets() {
    let capture = pollster::block_on(Capture::new(128, 64)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, 128, 64);
    configure(&mut renderer, &capture, &[material()], &[], &[]);
    let mut geometry = frame(0, 0.0);
    geometry.cameras.push(scene::EvaluatedCamera {
        node: 1,
        camera: 0,
        world: DMat4::from_translation(DVec3::new(0.0, 0.0, 300.0)),
    });
    for projection in [
        scene::Projection::Perspective {
            yfov: 0.9,
            aspect: Some(1.0),
            near_cm: 1.0,
            far_cm: None,
        },
        scene::Projection::Orthographic {
            xmag_cm: 150.0,
            ymag_cm: 150.0,
            near_cm: 1.0,
            far_cm: 1000.0,
        },
    ] {
        renderer.camera_definitions = vec![scene::Camera {
            name: "square".into(),
            projection,
        }];
        renderer
            .set_frame(&capture.device, &capture.queue, &geometry)
            .unwrap();
        let pixels = render_camera(&renderer, &capture, &Camera::default(), Some(0));
        let mut low = [128_u32, 64];
        let mut high = [0, 0];
        for y in 0..64 {
            for x in 0..128 {
                if pixels[((y * 128 + x) * 4) as usize] > 127 {
                    low[0] = low[0].min(x);
                    low[1] = low[1].min(y);
                    high[0] = high[0].max(x);
                    high[1] = high[1].max(y);
                }
            }
        }
        assert!(high[0] > low[0] && high[1] > low[1]);
        assert!(
            (high[0] - low[0]).abs_diff(high[1] - low[1]) <= 1,
            "authored square cannot stretch: {low:?} {high:?}"
        );
        assert!(
            low[0] >= 32 && high[0] < 96,
            "square camera needs pillarboxing"
        );
    }
}

#[test]
fn pbr_gpu_points_lines_and_unchanged_frames_reuse_geometry_buffers() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    configure(&mut renderer, &capture, &[material()], &[], &[]);
    let mut geometry = frame(0, 0.0);
    let solid = render(&mut renderer, &capture, &geometry);
    let uploads = renderer.uploads;
    let material_group = renderer.materials[0].group.clone();
    for _ in 0..3 {
        assert_eq!(render(&mut renderer, &capture, &geometry), solid);
    }
    assert_eq!(
        renderer.uploads, uploads,
        "camera redraws must not upload unchanged geometry"
    );
    assert_eq!(
        renderer.materials[0].group, material_group,
        "frame updates retain immutable textures/materials"
    );
    geometry.draws[0].vertices[0].position[0] += 10.0;
    render(&mut renderer, &capture, &geometry);
    assert_eq!(
        renderer.uploads,
        UploadCounts {
            vertices: uploads.vertices + 1,
            ..uploads
        },
        "deformation uploads vertices only"
    );
    let colored = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 0)
            .count()
    };
    geometry.draws[0].topology = Topology::Lines;
    geometry.draws[0].indices = Arc::from([0, 1, 1, 2, 2, 3, 3, 0]);
    let lines = render(&mut renderer, &capture, &geometry);
    assert!(colored(&lines) > 0 && colored(&lines) < colored(&solid) / 2);
    geometry.draws[0].topology = Topology::Points;
    geometry.draws[0].indices = Arc::from([0, 1, 2, 3]);
    let points = render(&mut renderer, &capture, &geometry);
    assert!(colored(&points) > 0 && colored(&points) < colored(&lines));
}

#[test]
fn pbr_gpu_invalid_frames_materials_and_camera_projections_fail_without_mutation() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = PbrSceneRenderer::new_for_test(&capture.device, SIZE, SIZE);
    configure(&mut renderer, &capture, &[material()], &[], &[]);
    let geometry = frame(0, 0.0);
    let before = render(&mut renderer, &capture, &geometry);
    let uploads = renderer.uploads;
    let mut invalid = geometry.clone();
    invalid.draws.push(invalid.draws[0].clone());
    invalid.draws[0].vertices[0].position[0] += 20.0;
    invalid.draws[1].vertices[0].normal[0] = f32::NAN;
    assert!(
        renderer
            .set_frame(&capture.device, &capture.queue, &invalid)
            .is_err()
    );
    assert_eq!(
        renderer.uploads, uploads,
        "preflight must precede partial uploads"
    );
    let mut camera = Camera::default();
    camera.set_view(View::Front);
    assert_eq!(render_camera(&renderer, &capture, &camera, None), before);
    let mut invalid_material = material();
    invalid_material.normal_scale = f32::MAX;
    assert!(
        textures::materials(
            &capture.device,
            &capture.queue,
            &renderer.material_layout,
            &[invalid_material],
            &[],
            &[]
        )
        .is_err()
    );
    renderer.frame_cameras = vec![scene::EvaluatedCamera {
        node: 7,
        camera: 0,
        world: DMat4::from_translation(DVec3::new(0.0, 0.0, 300.0)),
    }];
    for projection in [
        scene::Projection::Perspective {
            yfov: 0.0,
            aspect: Some(1.0),
            near_cm: 1.0,
            far_cm: None,
        },
        scene::Projection::Perspective {
            yfov: 0.8,
            aspect: Some(0.0),
            near_cm: 1.0,
            far_cm: None,
        },
        scene::Projection::Perspective {
            yfov: 0.8,
            aspect: None,
            near_cm: 10.0,
            far_cm: Some(1.0),
        },
        scene::Projection::Orthographic {
            xmag_cm: 0.0,
            ymag_cm: 1.0,
            near_cm: 0.0,
            far_cm: 10.0,
        },
    ] {
        renderer.camera_definitions = vec![scene::Camera {
            name: "invalid".into(),
            projection,
        }];
        assert!(renderer.imported_camera(0).unwrap_err().contains("node 7"));
    }
    renderer.camera_definitions[0].projection = scene::Projection::Perspective {
        yfov: 0.8,
        aspect: None,
        near_cm: 1.0,
        far_cm: None,
    };
    renderer.frame_cameras[0].world = DMat4::from_scale(DVec3::new(0.0, 1.0, 1.0));
    assert!(
        renderer.imported_camera(0).is_err(),
        "singular embedded camera must not panic"
    );
    assert_eq!(
        render_camera(&renderer, &capture, &camera, None),
        before,
        "bad imported camera does not disable free viewing"
    );
    assert!(draw_budget(usize::MAX, 3, Topology::Triangles, u64::MAX).is_err());
    assert!(draw_budget(3, 3, Topology::Triangles, 20).is_err());
    assert!(draw_budget(3, 2, Topology::Triangles, 4096).is_err());
    assert!(
        ViewTransform::from_bounds(Some(Bounds {
            min: DVec3::splat(-f64::MAX),
            max: DVec3::splat(f64::MAX)
        }))
        .validate()
        .is_err()
    );
}
