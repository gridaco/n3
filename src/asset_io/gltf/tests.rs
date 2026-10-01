use std::{collections::BTreeMap, sync::Arc};

use base64::Engine;
use serde_json::{Value, json};

use super::*;

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Memory {
    fn read(&self, path: &str, maximum: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path)
            .ok_or_else(|| format!("Missing resource {path}"))?;
        if bytes.len() > maximum {
            return Err("Resource exceeds requested limit".into());
        }
        Ok(bytes.clone())
    }
}

struct Builder {
    json: Value,
    bytes: Vec<u8>,
}
impl Builder {
    fn new() -> Self {
        Self {
            json: json!({"asset":{"version":"2.0"},"accessors":[],"bufferViews":[],"meshes":[],"nodes":[],"scenes":[{"nodes":[]}],"scene":0}),
            bytes: Vec::new(),
        }
    }
    fn view(&mut self, bytes: &[u8]) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        let views = self.json["bufferViews"].as_array_mut().unwrap();
        let index = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes.len()}));
        index
    }
    fn floats(&mut self, dimensions: &str, values: &[f32]) -> usize {
        let components = match dimensions {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => panic!("unsupported test shape"),
        };
        assert!(values.len().is_multiple_of(components));
        let bytes: Vec<_> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let view = self.view(&bytes);
        let min: Vec<_> = (0..components)
            .map(|i| {
                values
                    .chunks_exact(components)
                    .map(|chunk| chunk[i])
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let max: Vec<_> = (0..components)
            .map(|i| {
                values
                    .chunks_exact(components)
                    .map(|chunk| chunk[i])
                    .fold(f32::NEG_INFINITY, f32::max)
            })
            .collect();
        self.accessor(json!({"bufferView":view,"componentType":5126,"type":dimensions,"count":values.len()/components,"min":min,"max":max}))
    }
    fn accessor(&mut self, value: Value) -> usize {
        let accessors = self.json["accessors"].as_array_mut().unwrap();
        let index = accessors.len();
        accessors.push(value);
        index
    }
    fn triangle() -> Self {
        let mut builder = Self::new();
        let positions = builder.floats("VEC3", &[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
        builder.json["meshes"] = json!([{"primitives":[{"attributes":{"POSITION":positions}}]}]);
        builder.json["nodes"] = json!([{"mesh":0}]);
        builder.json["scenes"][0]["nodes"] = json!([0]);
        builder
    }
    fn prepare(&mut self) {
        self.json["buffers"] = json!([{"uri":"scene.bin","byteLength":self.bytes.len()}]);
    }
    fn asset(mut self) -> Result<SceneAsset> {
        self.prepare();
        load(
            "Fixture",
            &serde_json::to_vec(&self.json).unwrap(),
            &Memory(BTreeMap::from([("scene.bin".into(), self.bytes)])),
        )
    }
    fn animation(
        &mut self,
        interpolation: &str,
        path: &str,
        dimensions: &str,
        times: &[f32],
        values: &[f32],
    ) {
        let times = self.floats("SCALAR", times);
        let output = self.floats(dimensions, values);
        self.json["animations"] = json!([{"name":"Test clip","samplers":[{"input":times,"output":output,"interpolation":interpolation}],"channels":[{"sampler":0,"target":{"node":0,"path":path}}]}]);
    }
}

fn sample(asset: &SceneAsset, time: f32) -> Arc<EvaluatedScene> {
    asset
        .evaluate(0, Some(AnimationSample { clip: 0, time }))
        .unwrap()
}
fn position(frame: &EvaluatedScene, vertex: usize) -> DVec3 {
    DVec3::from_array(frame.draws[0].vertices[vertex].position)
}
fn approx(left: DVec3, right: DVec3) {
    assert!((left - right).length() < 1e-4, "{left:?} != {right:?}");
}

#[test]
fn hierarchy_units_scene_choice_reflections_and_static_cache_are_explicit() {
    let mut builder = Builder::triangle();
    builder.json["nodes"] = json!([{"translation":[1,2,3],"children":[1]}, {"mesh":0,"scale":[-2,3,1]}, {"mesh":0,"translation":[10,0,0]}]);
    builder.json["scenes"] = json!([{"nodes":[0]},{"nodes":[2]}]);
    let asset = builder.asset().unwrap();
    let frame = asset.evaluate(0, None).unwrap();
    assert!(Arc::ptr_eq(&frame, &asset.evaluate(0, None).unwrap()));
    assert_eq!(frame.draws.len(), 1);
    assert!(frame.draws[0].mirrored);
    approx(position(&frame, 0), DVec3::new(100., 200., 300.));
    approx(position(&frame, 1), DVec3::new(-100., 200., 300.));
    approx(frame.bounds.unwrap().max, DVec3::new(100., 500., 300.));
    let other = asset.evaluate(1, None).unwrap();
    approx(position(&other, 0), DVec3::new(1000., 0., 0.));
    assert!(!Arc::ptr_eq(&frame, &other));
    // Visiting another scene evicts the asset's old cached rest frame. External
    // consumers may keep their Arc, but the asset retains at most one frame.
    assert!(!Arc::ptr_eq(&frame, &asset.evaluate(0, None).unwrap()));
    assert!(asset.evaluate(2, None).is_err());
}

#[test]
fn each_core_primitive_topology_supports_indexed_and_nonindexed_data() {
    for (mode, count, topology, expanded) in [
        (0, 3, Topology::Points, 3),
        (1, 4, Topology::Lines, 4),
        (2, 4, Topology::Lines, 8),
        (3, 4, Topology::Lines, 6),
        (4, 3, Topology::Triangles, 3),
        (5, 4, Topology::Triangles, 6),
        (6, 4, Topology::Triangles, 6),
    ] {
        for indexed in [false, true] {
            let mut builder = Builder::new();
            let values = [0., 0., 0., 1., 0., 0., 0., 1., 0., 1., 1., 0.];
            let positions = builder.floats("VEC3", &values[..count * 3]);
            let mut primitive = json!({"attributes":{"POSITION":positions},"mode":mode});
            if indexed {
                let view = builder.view(
                    &(0..count as u16)
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                );
                let index = builder.accessor(
                    json!({"bufferView":view,"componentType":5123,"type":"SCALAR","count":count}),
                );
                primitive["indices"] = json!(index);
            }
            builder.json["meshes"] = json!([{"primitives":[primitive]}]);
            builder.json["nodes"] = json!([{"mesh":0}]);
            builder.json["scenes"][0]["nodes"] = json!([0]);
            let asset = builder.asset().unwrap();
            let frame = asset.evaluate(0, None).unwrap();
            assert_eq!(frame.draws[0].topology, topology);
            assert_eq!(frame.draws[0].indices.len(), expanded);
            if mode == 5 {
                assert_eq!(&*asset.meshes[0].primitives[0].indices, &[0, 1, 2, 2, 1, 3]);
            }
        }
    }
}

#[test]
fn glb_and_base64_resources_load_without_filesystem_access() {
    let mut builder = Builder::triangle();
    builder.prepare();
    builder.json["buffers"][0]
        .as_object_mut()
        .unwrap()
        .remove("uri");
    let mut json = serde_json::to_vec(&builder.json).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !builder.bytes.len().is_multiple_of(4) {
        builder.bytes.push(0);
    }
    let mut glb = Vec::new();
    for value in [
        0x46546C67,
        2,
        (12 + 8 + json.len() + 8 + builder.bytes.len()) as u32,
        json.len() as u32,
        0x4E4F534A,
    ] {
        glb.extend(value.to_le_bytes());
    }
    glb.extend(json);
    glb.extend((builder.bytes.len() as u32).to_le_bytes());
    glb.extend(0x004E4942_u32.to_le_bytes());
    glb.extend(&builder.bytes);
    let asset = load("GLB", &glb, &Memory::default()).unwrap();
    assert_eq!(asset.evaluate(0, None).unwrap().draws.len(), 1);
    builder.json["buffers"][0]["uri"] = json!(format!(
        "data:application/octet-stream;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(builder.bytes)
    ));
    let embedded = load(
        "glTF",
        &serde_json::to_vec(&builder.json).unwrap(),
        &Memory::default(),
    )
    .unwrap();
    assert_eq!(
        asset.evaluate(0, None).unwrap().draws[0].vertices,
        embedded.evaluate(0, None).unwrap().draws[0].vertices
    );
}

#[test]
fn sparse_zero_base_positions_and_interleaved_attributes_decode_exactly() {
    let mut builder = Builder::new();
    let indices = builder.view(&[1, 2]);
    let values = builder.view(
        &[1_f32, 0., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let positions=builder.accessor(json!({"componentType":5126,"type":"VEC3","count":3,"min":[0,0,0],"max":[1,1,0],"sparse":{"count":2,"indices":{"bufferView":indices,"componentType":5121},"values":{"bufferView":values}}}));
    builder.json["meshes"] = json!([{"primitives":[{"attributes":{"POSITION":positions}}]}]);
    builder.json["nodes"] = json!([{"mesh":0}]);
    builder.json["scenes"][0]["nodes"] = json!([0]);
    let asset = builder.asset().unwrap();
    approx(
        position(&asset.evaluate(0, None).unwrap(), 1),
        DVec3::X * 100.,
    );

    let mut builder = Builder::new();
    let data = [
        0_f32, 0., 0., 0., 0., 1., 1., 0., 0., 0., 0., 1., 0., 1., 0., 0., 0., 1.,
    ];
    let view = builder.view(
        &data
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    builder.json["bufferViews"][view]["byteStride"] = json!(24);
    let p=builder.accessor(json!({"bufferView":view,"componentType":5126,"type":"VEC3","count":3,"min":[0,0,0],"max":[1,1,0]}));
    let n = builder.accessor(
        json!({"bufferView":view,"byteOffset":12,"componentType":5126,"type":"VEC3","count":3}),
    );
    builder.json["meshes"] = json!([{"primitives":[{"attributes":{"POSITION":p,"NORMAL":n}}]}]);
    builder.json["nodes"] = json!([{"mesh":0}]);
    builder.json["scenes"][0]["nodes"] = json!([0]);
    let frame = builder.asset().unwrap().evaluate(0, None).unwrap();
    approx(position(&frame, 2), DVec3::Y * 100.);
    assert_eq!(frame.draws[0].vertices[0].normal, [0., 0., 1.]);
}

#[test]
fn matrix_accessors_allow_omitted_final_padding_and_keep_column_and_element_stride() {
    for (shape, component_type, columns, rows) in [
        ("MAT2", 5121, 2, 2),
        ("MAT3", 5121, 3, 3),
        ("MAT3", 5123, 3, 3),
    ] {
        for sparse in [false, true] {
            let mut builder = Builder::triangle();
            let components = columns * rows;
            let component_bytes: usize = if component_type == 5121 { 1 } else { 2 };
            let column_bytes = rows * component_bytes;
            let column_stride = column_bytes.div_ceil(4) * 4;
            let element_stride = columns * column_stride;
            let mut matrices = Vec::new();
            for matrix in 0..2 {
                for column in 0..columns {
                    for row in 0..rows {
                        let value = (matrix * components + column * rows + row + 1) as u16;
                        matrices.extend_from_slice(&value.to_le_bytes()[..component_bytes]);
                    }
                    matrices.resize(matrices.len() + column_stride - column_bytes, 0);
                }
            }
            matrices.truncate(element_stride + (columns - 1) * column_stride + column_bytes);
            let sparse_indices = sparse.then(|| builder.view(&[1, 3]));
            let view = builder.view(&matrices);
            let mut accessor = json!({"componentType":component_type,"type":shape,"count":2});
            let expected: Vec<f64> = if let Some(indices) = sparse_indices {
                accessor["count"] = json!(4);
                accessor["sparse"] = json!({"count":2,"indices":{"bufferView":indices,"componentType":5121},"values":{"bufferView":view}});
                (0..4)
                    .flat_map(|matrix| {
                        (0..components).map(move |component| match matrix {
                            1 => (component + 1) as f64,
                            3 => (components + component + 1) as f64,
                            _ => 0.,
                        })
                    })
                    .collect()
            } else {
                accessor["bufferView"] = json!(view);
                (1..=components * 2).map(|value| value as f64).collect()
            };
            let index = builder.accessor(accessor);
            builder.prepare();
            let parsed =
                ::gltf::Gltf::from_slice(&serde_json::to_vec(&builder.json).unwrap()).unwrap();
            assert_eq!(
                accessors::read(
                    parsed.document.accessors().nth(index).unwrap(),
                    std::slice::from_ref(&builder.bytes)
                )
                .unwrap(),
                expected,
                "{shape}/{component_type}, sparse={sparse}"
            );
            let mut truncated = Builder {
                json: builder.json.clone(),
                bytes: builder.bytes.clone(),
            };
            truncated.json["bufferViews"][view]["byteLength"] = json!(matrices.len() - 1);
            assert!(
                truncated
                    .asset()
                    .unwrap_err()
                    .contains("exceeds its buffer view")
            );
            builder.asset().unwrap();
        }
    }
}

#[test]
fn unsafe_resource_paths_and_symbolic_links_are_rejected() {
    let memory = Memory::default();
    for uri in [
        "../secret.bin",
        "%2e%2e/secret.bin",
        "/tmp/secret.bin",
        "https://example.test/a.bin",
        "file:local.bin",
        "C:\\secret.bin",
        "safe/%2e%2e/secret.bin",
        "%2fetc/passwd",
        "a.bin?x=1",
        "a%00.bin",
    ] {
        assert!(
            resources::uri_bytes(uri, &memory)
                .unwrap_err()
                .contains("relative"),
            "{uri}"
        );
    }
    let memory = Memory(BTreeMap::from([("mesh data.bin".into(), vec![1, 2, 3])]));
    assert_eq!(
        resources::uri_bytes("mesh%20data.bin", &memory).unwrap(),
        vec![1, 2, 3]
    );
    #[cfg(unix)]
    {
        let root = std::env::temp_dir().join(format!(
            "n3-scene-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("real.bin"), [1, 2, 3]).unwrap();
        std::os::unix::fs::symlink(root.join("real.bin"), root.join("link.bin")).unwrap();
        let resolver = FileResolver::new(&root).unwrap();
        assert!(
            resolver
                .read("link.bin", 1024)
                .unwrap_err()
                .contains("symbolic")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn malformed_accessor_spans_sparse_indices_and_nonfinite_values_fail() {
    let mut builder = Builder::triangle();
    builder.json["accessors"][0]["byteOffset"] = json!(1);
    assert!(builder.asset().is_err());
    let mut builder = Builder::triangle();
    builder.json["accessors"][0]["count"] = json!(4);
    assert!(builder.asset().is_err());
    let mut builder = Builder::triangle();
    builder.bytes[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(builder.asset().unwrap_err().contains("nonfinite"));
    let mut builder = Builder::triangle();
    let indices = builder.view(&[1, 1]);
    let values = builder.view(&[0_u8; 24]);
    builder.json["accessors"][0]["sparse"] = json!({"count":2,"indices":{"bufferView":indices,"componentType":5121},"values":{"bufferView":values}});
    assert!(builder.asset().unwrap_err().contains("increasing"));
    let mut builder = Builder::triangle();
    builder.json["accessors"][0]["byteOffset"] = json!(u64::MAX);
    assert!(builder.asset().is_err());
}

#[test]
fn required_extensions_fail_and_optional_extensions_are_explicit_warnings() {
    let mut builder = Builder::triangle();
    builder.json["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
    builder.json["extensionsUsed"] = json!(["KHR_draco_mesh_compression"]);
    assert!(builder.asset().is_err());
    let mut builder = Builder::triangle();
    builder.json["extensionsUsed"] = json!(["EXT_future_appearance"]);
    assert!(
        builder
            .asset()
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.contains("EXT_future_appearance"))
    );
}

#[test]
fn hierarchy_cycles_multiple_parents_and_invalid_indices_fail_without_panics() {
    let mut builder = Builder::triangle();
    builder.json["nodes"][0]["children"] = json!([0]);
    assert!(builder.asset().unwrap_err().contains("cycle"));
    let mut builder = Builder::triangle();
    builder.json["nodes"] = json!([{"children":[2]},{"children":[2]},{"mesh":0}]);
    assert!(builder.asset().unwrap_err().contains("parents"));
    let mut builder = Builder::triangle();
    let view = builder.view(&[0, 1, 8]);
    let accessor =
        builder.accessor(json!({"bufferView":view,"componentType":5121,"type":"SCALAR","count":3}));
    builder.json["meshes"][0]["primitives"][0]["indices"] = json!(accessor);
    assert!(builder.asset().unwrap_err().contains("index exceeds"));
}

#[test]
fn linear_and_step_translations_use_absolute_time_and_clamp_without_accumulation() {
    for interpolation in ["LINEAR", "STEP"] {
        let mut builder = Builder::triangle();
        builder.animation(
            interpolation,
            "translation",
            "VEC3",
            &[2., 4.],
            &[1., 0., 0., 3., 0., 0.],
        );
        let asset = builder.asset().unwrap();
        assert_eq!(asset.animations[0].start, 2.);
        assert_eq!(asset.animations[0].duration, 2.);
        approx(
            position(&sample(&asset, 3.), 0),
            DVec3::X
                * if interpolation == "LINEAR" {
                    200.
                } else {
                    100.
                },
        );
        approx(position(&sample(&asset, 100.), 0), DVec3::X * 300.);
        approx(position(&sample(&asset, 0.), 0), DVec3::X * 100.);
        approx(position(&asset.evaluate(0, None).unwrap(), 0), DVec3::ZERO);
        assert!(
            asset
                .evaluate(
                    0,
                    Some(AnimationSample {
                        clip: 0,
                        time: f32::NAN
                    })
                )
                .is_err()
        );
    }
}

#[test]
fn cubic_translation_uses_tangents_times_segment_duration() {
    let mut builder = Builder::triangle();
    builder.animation(
        "CUBICSPLINE",
        "translation",
        "VEC3",
        &[0., 2.],
        &[
            0., 0., 0., 0., 0., 0., 2., 0., 0., 0., 0., 0., 2., 0., 0., 0., 0., 0.,
        ],
    );
    let asset = builder.asset().unwrap();
    approx(position(&sample(&asset, 1.), 0), DVec3::X * 150.);
}

#[test]
fn quaternion_linear_uses_shortest_arc_and_cubic_normalizes() {
    let mut builder = Builder::triangle();
    builder.animation(
        "LINEAR",
        "rotation",
        "VEC4",
        &[0., 1.],
        &[0., 0., 0., 1., 0., 0., -1., 0.],
    );
    let asset = builder.asset().unwrap();
    approx(position(&sample(&asset, 0.5), 1), DVec3::NEG_Y * 100.);
    let mut builder = Builder::triangle();
    builder.animation(
        "CUBICSPLINE",
        "rotation",
        "VEC4",
        &[0., 1.],
        &[
            0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0.,
            0., 0.,
        ],
    );
    let asset = builder.asset().unwrap();
    approx(position(&sample(&asset, 0.5), 1), DVec3::Y * 100.);
}

#[test]
fn morph_targets_use_mesh_or_node_weights_and_animate_without_changing_asset() {
    let mut builder = Builder::triangle();
    let target = builder.floats("VEC3", &[0., 0., 1., 0., 0., 1., 0., 0., 1.]);
    builder.json["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":target}]);
    builder.json["meshes"][0]["weights"] = json!([0.25]);
    builder.json["nodes"][0]["weights"] = json!([0.5]);
    builder.animation("LINEAR", "weights", "SCALAR", &[0., 1.], &[0., 1.]);
    let asset = builder.asset().unwrap();
    approx(
        position(&asset.evaluate(0, None).unwrap(), 0),
        DVec3::Z * 50.,
    );
    approx(position(&sample(&asset, 0.25), 0), DVec3::Z * 25.);
    assert_eq!(asset.meshes[0].weights, vec![0.25]);
}

#[test]
fn skinning_uses_joint_hierarchy_inverse_bind_and_normalized_weights() {
    let mut builder = Builder::triangle();
    let joint_view = builder.view(&[0_u8; 12]);
    let joints = builder
        .accessor(json!({"bufferView":joint_view,"componentType":5121,"type":"VEC4","count":3}));
    let weights = builder.floats("VEC4", &[2., 0., 0., 0., 2., 0., 0., 0., 2., 0., 0., 0.]);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = json!(joints);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(weights);
    let inverse = builder.floats(
        "MAT4",
        &DMat4::from_translation(DVec3::NEG_X)
            .as_mat4()
            .to_cols_array(),
    );
    builder.json["skins"] = json!([{"joints":[2],"inverseBindMatrices":inverse}]);
    builder.json["nodes"] = json!([{"mesh":0,"skin":0,"translation":[99,0,0]}, {"translation":[1,0,0],"children":[2]}, {"translation":[0,2,0]}]);
    builder.json["scenes"][0]["nodes"] = json!([0, 1]);
    builder.animation(
        "LINEAR",
        "translation",
        "VEC3",
        &[0., 1.],
        &[0., 2., 0., 0., 4., 0.],
    );
    builder.json["animations"][0]["channels"][0]["target"]["node"] = json!(2);
    let asset = builder.asset().unwrap();
    approx(
        position(&asset.evaluate(0, None).unwrap(), 0),
        DVec3::Y * 200.,
    );
    approx(position(&sample(&asset, 0.5), 0), DVec3::Y * 300.);
}

#[test]
fn animated_matrix_nodes_bad_key_times_and_output_counts_reject() {
    let mut builder = Builder::triangle();
    builder.json["nodes"][0]["matrix"] = json!(DMat4::IDENTITY.to_cols_array());
    builder.animation("LINEAR", "translation", "VEC3", &[0., 1.], &[0.; 6]);
    assert!(builder.asset().unwrap_err().contains("matrix"));
    let mut builder = Builder::triangle();
    builder.animation("LINEAR", "translation", "VEC3", &[1., 1.], &[0.; 6]);
    assert!(builder.asset().unwrap_err().contains("increasing"));
    let mut builder = Builder::triangle();
    builder.animation("CUBICSPLINE", "translation", "VEC3", &[0., 1.], &[0.; 6]);
    assert!(builder.asset().unwrap_err().contains("output count"));
}

#[test]
fn png_jpeg_material_channels_samplers_and_all_texture_transforms_survive() {
    let mut builder = Builder::triangle();
    let uv = builder.floats("VEC2", &[0., 0., 1., 0., 0., 1.]);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(uv);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_1"] = json!(uv);
    let rgb = image::RgbImage::from_pixel(2, 2, image::Rgb([200, 100, 50]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rgb.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&rgb)
        .unwrap();
    builder.json["images"] = json!([
        {"uri":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(png.into_inner()))},
        {"uri":format!("data:image/jpeg;base64,{}",base64::engine::general_purpose::STANDARD.encode(jpeg))},
    ]);
    builder.json["samplers"] =
        json!([{"magFilter":9728,"minFilter":9986,"wrapS":33071,"wrapT":33648}]);
    builder.json["textures"] = json!([{"source":0,"sampler":0},{"source":1}]);
    let transform = json!({"offset":[0.25,0.5],"scale":[2,3],"rotation":0.5,"texCoord":1});
    let texture = json!({"index":0,"extensions":{"KHR_texture_transform":transform}});
    builder.json["extensionsUsed"] = json!(["KHR_texture_transform", "KHR_materials_unlit"]);
    builder.json["materials"] = json!([{
        "pbrMetallicRoughness":{"baseColorFactor":[0.2,0.3,0.4,0.5],"metallicFactor":0.7,"roughnessFactor":0.2,"baseColorTexture":texture,"metallicRoughnessTexture":{"index":1}},
        "normalTexture":{"index":0,"scale":0.75,"extensions":{"KHR_texture_transform":transform}},
        "occlusionTexture":{"index":0,"strength":0.4,"extensions":{"KHR_texture_transform":transform}},
        "emissiveTexture":texture,"emissiveFactor":[0.1,0.2,0.3],"alphaMode":"MASK","alphaCutoff":0.3,"doubleSided":true,"extensions":{"KHR_materials_unlit":{}}
    }]);
    builder.json["meshes"][0]["primitives"][0]["material"] = json!(0);
    let asset = builder.asset().unwrap();
    assert_eq!((asset.images[0].width, asset.images[1].height), (2, 2));
    assert_eq!(asset.images[0].rgba8.len(), 16);
    let material = &asset.materials[0];
    assert_eq!(material.base_color, [0.2, 0.3, 0.4, 0.5]);
    assert_eq!(material.alpha_mode, AlphaMode::Mask);
    assert!(material.double_sided && material.unlit);
    for texture in [
        material.base_color_texture,
        material.normal_texture,
        material.occlusion_texture,
        material.emissive_texture,
    ] {
        let texture = texture.unwrap();
        assert_eq!(texture.tex_coord, 1);
        assert_eq!(texture.transform.offset, [0.25, 0.5]);
        assert_eq!(texture.transform.scale, [2., 3.]);
        assert_eq!(texture.transform.rotation, 0.5);
    }
    let sampler = asset.textures[0].sampler;
    assert_eq!(sampler.wrap_s, Wrap::Clamp);
    assert_eq!(sampler.wrap_t, Wrap::Mirror);
    assert_eq!(sampler.mag, Filter::Nearest);
    assert_eq!(sampler.min, Filter::Nearest);
    assert_eq!(sampler.mipmap, Some(Filter::Linear));
}

#[test]
fn cameras_and_punctual_lights_use_node_instances_and_centimeters() {
    let mut builder = Builder::triangle();
    builder.json["cameras"] = json!([{"type":"perspective","perspective":{"yfov":1,"znear":0.1}},{"type":"orthographic","orthographic":{"xmag":2,"ymag":3,"znear":0,"zfar":10}}]);
    builder.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    builder.json["extensionsRequired"] = json!(["KHR_lights_punctual"]);
    builder.json["extensions"] = json!({"KHR_lights_punctual":{"lights":[{"type":"point","intensity":20,"range":5},{"type":"spot","spot":{"innerConeAngle":0.1,"outerConeAngle":0.5}},{"type":"directional"}]}});
    builder.json["nodes"] = json!([{"mesh":0},{"camera":0,"translation":[0,1,3]},{"camera":1},{"translation":[2,3,4],"extensions":{"KHR_lights_punctual":{"light":0}}},{"extensions":{"KHR_lights_punctual":{"light":1}}},{"extensions":{"KHR_lights_punctual":{"light":2}}}]);
    builder.json["scenes"][0]["nodes"] = json!([0, 1, 2, 3, 4, 5]);
    let asset = builder.asset().unwrap();
    let frame = asset.evaluate(0, None).unwrap();
    assert_eq!(frame.cameras.len(), 2);
    assert_eq!(frame.lights.len(), 3);
    assert_eq!(frame.cameras[0].node, 1);
    assert_eq!(frame.lights[0].node, 3);
    approx(
        frame.cameras[0].world.w_axis.truncate(),
        DVec3::new(0., 100., 300.),
    );
    approx(frame.lights[0].position_cm, DVec3::new(200., 300., 400.));
    assert_eq!(frame.lights[0].direction, DVec3::NEG_Z);
    assert_eq!(asset.lights[0].range_cm, Some(500.));
    assert!(matches!(
        asset.cameras[0].projection,
        Projection::Perspective {
            near_cm: 10.,
            far_cm: None,
            ..
        }
    ));
    assert!(matches!(
        asset.cameras[1].projection,
        Projection::Orthographic {
            xmag_cm: 200.,
            ymag_cm: 300.,
            far_cm: 1000.,
            ..
        }
    ));
}

#[test]
fn normals_use_inverse_transpose_and_reflected_tangents_keep_handedness() {
    let mut builder = Builder::triangle();
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let normals = builder.floats("VEC3", &[h, h, 0., h, h, 0., h, h, 0.]);
    let tangents = builder.floats("VEC4", &[0., 0., 1., -1., 0., 0., 1., -1., 0., 0., 1., -1.]);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(normals);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(tangents);
    builder.json["nodes"][0]["scale"] = json!([-2, 1, 1]);
    let asset = builder.asset().unwrap();
    let frame = asset.evaluate(0, None).unwrap();
    let vertex = frame.draws[0].vertices[0];
    approx(
        DVec3::from_array(vertex.normal.map(f64::from)),
        DVec3::new(-0.5, 1., 0.).normalize(),
    );
    assert_eq!(vertex.tangent, [0., 0., 1., 1.]);
}

#[test]
fn singular_animation_scale_remains_finite_and_reflections_are_evaluated() {
    let mut builder = Builder::triangle();
    builder.animation(
        "LINEAR",
        "scale",
        "VEC3",
        &[0., 1.],
        &[1., 1., 1., -1., 2., 1.],
    );
    let asset = builder.asset().unwrap();
    let collapsed = sample(&asset, 0.5);
    assert!(
        collapsed.draws[0]
            .vertices
            .iter()
            .all(|vertex| vertex.normal.iter().all(|value| value.is_finite()))
    );
    assert!(sample(&asset, 1.).draws[0].mirrored);
}

#[test]
fn invalid_counts_and_image_payloads_fail_before_scene_installation() {
    let mut builder = Builder::triangle();
    builder.json["images"] = json!([{"uri":"data:image/png;base64,AAECAw=="}]);
    assert!(builder.asset().is_err());
    let mut builder = Builder::triangle();
    builder.json["accessors"][0]["count"] = json!(usize::MAX);
    assert!(builder.asset().is_err());
    let mut builder = Builder::triangle();
    builder.json["meshes"][0]["primitives"][0]["mode"] = json!(1);
    assert!(builder.asset().unwrap_err().contains("topology"));
}

#[test]
fn shared_accessor_expansion_is_bounded_before_resource_reads_or_allocations() {
    struct NoRead;
    impl ResourceResolver for NoRead {
        fn read(&self, _: &str, _: usize) -> Result<Vec<u8>> {
            panic!("budget preflight must precede external resource reads")
        }
    }
    let mut builder = Builder::triangle();
    builder.json["accessors"][0]["count"] = json!(399_999);
    builder.json["meshes"][0]["primitives"][0]["targets"] = json!(vec![json!({"POSITION":0}); 64]);
    builder.prepare();
    assert!(
        load(
            "morph amplification",
            &serde_json::to_vec(&builder.json).unwrap(),
            &NoRead
        )
        .unwrap_err()
        .contains("budget")
    );

    let mut builder = Builder::triangle();
    builder.animation("LINEAR", "translation", "VEC3", &[0., 1.], &[0.; 6]);
    builder.json["accessors"][1]["count"] = json!(4_000_000);
    builder.json["accessors"][2]["count"] = json!(4_000_000);
    builder.json["nodes"] = json!(vec![json!({}); 8]);
    builder.json["animations"][0]["channels"] = json!(
        (0..8)
            .map(|node| json!({"sampler":0,"target":{"node":node,"path":"translation"}}))
            .collect::<Vec<_>>()
    );
    builder.prepare();
    assert!(
        load(
            "animation amplification",
            &serde_json::to_vec(&builder.json).unwrap(),
            &NoRead
        )
        .unwrap_err()
        .contains("budget")
    );

    assert!(
        geometry::topology_count(::gltf::mesh::Mode::TriangleStrip, geometry::MAX_INDICES).is_err()
    );
    assert!(geometry::topology_count(::gltf::mesh::Mode::TriangleFan, usize::MAX).is_err());
    assert!(geometry::topology_count(::gltf::mesh::Mode::LineLoop, usize::MAX).is_err());
    let mut builder = Builder::triangle();
    builder.json["meshes"][0]["primitives"] = json!(vec![
        json!({"attributes":{"POSITION":0}});
        crate::scene::budgets::MAX_PRIMITIVES + 1
    ]);
    assert!(builder.asset().unwrap_err().contains("primitive count"));
}

#[test]
fn instanced_draw_and_deformation_work_limits_fail_before_evaluating_geometry() {
    let mut builder = Builder::triangle();
    builder.json["meshes"][0]["primitives"] = json!(vec![json!({"attributes":{"POSITION":0}}); 2]);
    builder.json["nodes"] = json!(vec![json!({"mesh":0}); 4096]);
    builder.json["scenes"][0]["nodes"] = json!((0..4096).collect::<Vec<_>>());
    assert!(builder.asset().unwrap_err().contains("draw count"));
}

#[test]
fn camera_light_overflow_and_active_light_limits_fail_explicitly() {
    let mut builder = Builder::triangle();
    builder.json["cameras"] = json!([{"type":"perspective","perspective":{"yfov":1,"znear":1e38}}]);
    assert!(builder.asset().unwrap_err().contains("centimeter"));
    let mut builder = Builder::triangle();
    builder.json["cameras"] =
        json!([{"type":"orthographic","orthographic":{"xmag":1e38,"ymag":3,"znear":0,"zfar":10}}]);
    assert!(builder.asset().unwrap_err().contains("centimeter"));
    let mut builder = Builder::triangle();
    builder.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    builder.json["extensions"] =
        json!({"KHR_lights_punctual":{"lights":[{"type":"point","range":1e38}]}});
    assert!(builder.asset().unwrap_err().contains("centimeter"));
    let mut builder = Builder::triangle();
    builder.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    builder.json["extensions"] = json!({"KHR_lights_punctual":{"lights":[{"type":"point"}]}});
    builder.json["nodes"] = json!(vec![
        json!({"extensions":{"KHR_lights_punctual":{"light":0}}});
        MAX_ACTIVE_LIGHTS + 1
    ]);
    builder.json["scenes"][0]["nodes"] = json!((0..MAX_ACTIVE_LIGHTS + 1).collect::<Vec<_>>());
    assert!(
        builder
            .asset()
            .unwrap_err()
            .contains("active punctual light budget")
    );
}

fn skinned_triangle() -> Builder {
    let mut builder = Builder::triangle();
    let view = builder.view(&[0; 12]);
    let joints =
        builder.accessor(json!({"bufferView":view,"componentType":5121,"type":"VEC4","count":3}));
    let weights = builder.floats("VEC4", &[1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.]);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = json!(joints);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(weights);
    builder.json["skins"] = json!([{"joints":[2]}]);
    builder.json["nodes"] = json!([{"children":[1,2],"scale":[-1,1,1]},{"mesh":0,"skin":0},{}]);
    builder.json["scenes"][0]["nodes"] = json!([0]);
    builder
}

#[test]
fn skins_accept_extra_bind_matrices_validate_affine_common_root_and_reflected_parent() {
    let mut builder = skinned_triangle();
    let matrices = builder.floats("MAT4", &glam::Mat4::IDENTITY.to_cols_array().repeat(2));
    builder.json["skins"][0]["inverseBindMatrices"] = json!(matrices);
    let asset = builder.asset().unwrap();
    assert_eq!(asset.skins[0].inverse_bind.len(), 1);
    let frame = asset.evaluate(0, None).unwrap();
    assert!(frame.draws[0].mirrored);
    approx(position(&frame, 1), DVec3::NEG_X * 100.);

    let mut builder = skinned_triangle();
    let mut matrix = glam::Mat4::IDENTITY.to_cols_array();
    matrix[3] = 0.5;
    let matrices = builder.floats("MAT4", &matrix);
    builder.json["skins"][0]["inverseBindMatrices"] = json!(matrices);
    assert!(builder.asset().unwrap_err().contains("affine"));

    let mut builder = skinned_triangle();
    builder.json["skins"][0]["joints"] = json!([0, 2]);
    builder.json["nodes"][0]["children"] = json!([1]);
    builder.json["scenes"][0]["nodes"] = json!([0, 2]);
    assert!(builder.asset().unwrap_err().contains("common ancestor"));
}

#[test]
#[ignore = "manual informational CPU timing; no timing thresholds"]
fn fixture_evaluation_timings() {
    use std::{hint::black_box, time::Instant};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/gltf");
    for path in [
        "WaterBottle/glTF-Binary/WaterBottle.glb",
        "SimpleSkin/glTF/SimpleSkin.gltf",
        "AnimatedMorphCube/glTF/AnimatedMorphCube.gltf",
    ] {
        let started = Instant::now();
        let asset = load_path(&root.join(path)).unwrap();
        let cold_ms = started.elapsed().as_secs_f64() * 1000.;
        let rest = asset.evaluate(asset.default_scene, None).unwrap();
        let count: usize = rest.draws.iter().map(|draw| draw.vertices.len()).sum();
        let started = Instant::now();
        for _ in 0..10_000 {
            assert!(Arc::ptr_eq(
                &rest,
                &black_box(asset.evaluate(asset.default_scene, None).unwrap())
            ));
        }
        let cached_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        let mut frames = 0;
        for (clip, animation) in asset.animations.iter().enumerate() {
            for i in 0..120 {
                let frame = asset
                    .evaluate(
                        asset.default_scene,
                        Some(AnimationSample {
                            clip,
                            time: animation.start + animation.duration * i as f32 / 119.,
                        }),
                    )
                    .unwrap();
                assert!(frame.node_world.iter().all(|matrix| matrix.is_finite()));
                for vertex in frame.draws.iter().flat_map(|draw| &draw.vertices) {
                    assert!(vertex.position.iter().all(|value| value.is_finite()));
                    assert!(
                        vertex
                            .normal
                            .iter()
                            .chain(&vertex.tangent)
                            .all(|value| value.is_finite())
                    );
                }
                black_box(frame);
                frames += 1;
            }
        }
        eprintln!(
            "{path}: vertices={count} draws={} cold_decode_rest_ms={cold_ms:.3} cached_10000_ms={cached_ms:.3} sampled_frames={frames} sampled_total_ms={:.3}",
            rest.draws.len(),
            started.elapsed().as_secs_f64() * 1000.
        );
    }
}

#[test]
fn import_normalizes_all_spatial_inputs_once_without_scaling_directions() {
    let mut builder = Builder::triangle();
    let normals = builder.floats("VEC3", &[0., 0., 1., 0., 0., 1., 0., 0., 1.]);
    let tangents = builder.floats("VEC4", &[1., 0., 0., -1., 1., 0., 0., -1., 1., 0., 0., -1.]);
    let positions = builder.floats("VEC3", &[0.1, 0.2, 0.3, 0.1, 0.2, 0.3, 0.1, 0.2, 0.3]);
    let directions = builder.floats("VEC3", &[0.1, 0., 0., 0.1, 0., 0., 0.1, 0., 0.]);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(normals);
    builder.json["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(tangents);
    builder.json["meshes"][0]["primitives"][0]["targets"] =
        json!([{"POSITION":positions,"NORMAL":directions,"TANGENT":directions}]);
    builder.json["nodes"][0]["translation"] = json!([1, 2, 3]);
    let asset = builder.asset().unwrap();
    let primitive = &asset.meshes[0].primitives[0];
    assert_eq!(primitive.vertices[1].position, [100., 0., 0.]);
    assert_eq!(primitive.vertices[0].normal, [0., 0., 1.]);
    assert_eq!(primitive.vertices[0].tangent, [1., 0., 0., -1.]);
    assert_eq!(
        primitive.morphs[0].positions[0],
        [0.1_f32, 0.2, 0.3].map(|v| f64::from(v) * 100.)
    );
    assert_eq!(primitive.morphs[0].normals[0], [0.1, 0., 0.]);
    assert_eq!(primitive.morphs[0].tangents[0], [0.1, 0., 0.]);
    let Transform::Trs {
        translation, scale, ..
    } = asset.nodes[0].transform
    else {
        panic!()
    };
    assert_eq!(translation, DVec3::new(100., 200., 300.));
    assert_eq!(scale, DVec3::ONE);
}

#[test]
fn matrix_and_inverse_bind_unit_changes_preserve_basis_and_skin_result() {
    let mut builder = skinned_triangle();
    let matrix = glam::Mat4::from_scale_rotation_translation(
        glam::Vec3::new(-2., 3., 1.),
        glam::Quat::IDENTITY,
        glam::Vec3::new(1., 2., 3.),
    );
    builder.json["nodes"][0] = json!({"children":[1,2],"matrix":matrix.to_cols_array()});
    let inverse = builder.floats(
        "MAT4",
        &glam::Mat4::from_translation(glam::Vec3::new(-0.5, 0., 0.)).to_cols_array(),
    );
    builder.json["skins"][0]["inverseBindMatrices"] = json!(inverse);
    let asset = builder.asset().unwrap();
    let Transform::Matrix(imported) = asset.nodes[0].transform else {
        panic!()
    };
    assert_eq!(imported.x_axis.x, -2.);
    assert_eq!(imported.y_axis.y, 3.);
    assert_eq!(imported.w_axis.truncate(), DVec3::new(100., 200., 300.));
    assert_eq!(asset.skins[0].inverse_bind[0].w_axis.x, -50.);
    let frame = asset.evaluate(0, None).unwrap();
    approx(position(&frame, 0), DVec3::new(200., 200., 300.));
    approx(position(&frame, 1), DVec3::new(0., 200., 300.));
    assert!(frame.draws[0].mirrored);
}

#[test]
fn translation_keys_and_both_hermite_derivatives_convert_to_f64_centimeters() {
    let mut builder = Builder::triangle();
    builder.animation(
        "CUBICSPLINE",
        "translation",
        "VEC3",
        &[0., 2.],
        &[
            1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 14., 15., 16., 17., 18.,
        ],
    );
    let asset = builder.asset().unwrap();
    let channel = &asset.animations[0].channels[0];
    assert_eq!(channel.interpolation, Interpolation::CubicHermite);
    assert_eq!(channel.times, vec![0., 2.]);
    assert_eq!(channel.values, vec![400., 500., 600., 1300., 1400., 1500.]);
    assert_eq!(
        channel.in_tangents,
        vec![100., 200., 300., 1000., 1100., 1200.]
    );
    assert_eq!(
        channel.out_tangents,
        vec![700., 800., 900., 1600., 1700., 1800.]
    );
    approx(
        position(&sample(&asset, 1.), 0),
        DVec3::new(775., 875., 975.),
    );

    let mut builder = Builder::triangle();
    builder.animation(
        "LINEAR",
        "translation",
        "VEC3",
        &[0., 1.],
        &[0., 0., 0., f32::MAX, 0., 0.],
    );
    let asset = builder.asset().unwrap();
    let expected = f64::from(f32::MAX) * 100.;
    assert_eq!(asset.animations[0].channels[0].values[3], expected);
    assert_eq!(position(&sample(&asset, 1.), 0).x, expected);
    assert!(expected.is_finite());
}
