//! Isolate attachment color conversion from scene evaluation, lighting, textures,
//! compositing and media encoding. Fingerprints diagnose CPU-host differences;
//! ordinary conversion bounds here do not relax the exact guide comparison.
use crate::doc_capture::Capture;
use sha2::{Digest, Sha256};
use std::{sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

const CONSTANTS: [f32; 11] = [
    0.0, 0.00001, 0.001, 0.0031308, 0.003131, 0.01, 0.1, 0.18, 0.21404114, 0.5, 1.0,
];
const SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> samples: array<vec4<f32>>;
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4(corners[index], 0.0, 1.0);
}
@fragment fn linear(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return samples[u32(position.x)];
}
@fragment fn explicit_transfer(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let value = samples[u32(position.x)];
    let encoded = select(1.055 * pow(value.rgb, vec3(1.0 / 2.4)) - vec3(0.055),
                         12.92 * value.rgb, value.rgb <= vec3(0.0031308));
    return vec4(encoded, value.a);
}
"#;

fn reference(linear: f32) -> f64 {
    let linear = f64::from(linear);
    if linear <= 0.0031308 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn render(
    capture: &Capture,
    samples: &[[f32; 4]],
    format: wgpu::TextureFormat,
    entry: &str,
) -> Vec<u8> {
    let device = &capture.device;
    let width = u32::try_from(samples.len()).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("n3 color transfer probe"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("n3 color transfer probe"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("n3 exact linear transfer inputs"),
        contents: bytemuck::cast_slice(samples),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("n3 exact linear transfer inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: input.as_entire_binding(),
        }],
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("n3 color transfer target"),
        size: wgpu::Extent3d {
            width,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let row_bytes = width * 4;
    let stride =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("n3 raw color transfer pixels"),
        size: u64::from(stride),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("n3 color transfer probe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(1),
            },
        },
        target.size(),
    );
    let submission = capture.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let result = mapped[..row_bytes as usize].to_vec();
    drop(mapped);
    readback.unmap();
    result
}

#[test]
fn imported_render_fingerprints_color_transfer() {
    let capture = pollster::block_on(Capture::new(1, 1)).unwrap();
    let mut ramp: Vec<_> = (0..4096).map(|index| index as f32 / 4095.0).collect();
    ramp.extend(CONSTANTS);
    ramp.sort_by(f32::total_cmp);
    let samples: Vec<_> = ramp.iter().map(|&value| [value; 4]).collect();
    let input_digest = Sha256::digest(bytemuck::cast_slice(&samples));
    println!("color-transfer adapter: {}", capture.adapter);
    println!(
        "color-transfer input: samples={} sha256={input_digest:x}",
        samples.len()
    );
    let srgb = render(
        &capture,
        &samples,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        "linear",
    );
    let explicit = render(
        &capture,
        &samples,
        wgpu::TextureFormat::Rgba8Unorm,
        "explicit_transfer",
    );
    for (name, pixels) in [("attachment-srgb", &srgb), ("explicit-unorm", &explicit)] {
        let digest = Sha256::digest(pixels);
        println!("color-transfer {name}: rgba-sha256={digest:x}");
        let rgba = pixels.as_chunks::<4>().0;
        let dense_decreases = rgba
            .windows(2)
            .filter(|pair| pair[0][0] > pair[1][0])
            .count();
        println!("color-transfer {name}: dense-ramp-decreases={dense_decreases}");
        let mut maximum_error = 0.0_f64;
        for (index, pixel) in rgba.iter().enumerate() {
            let expected = reference(ramp[index]) * 255.0;
            let error = (f64::from(pixel[0]) - expected).abs();
            maximum_error = maximum_error.max(error);
            // One 8-bit code value bounds transfer approximation plus output
            // quantization. This catches missing/double gamma without assuming
            // different implementations choose identical approximations.
            assert!(
                error <= 1.0,
                "{name} at {}: {pixel:?}, reference={expected}",
                ramp[index]
            );
            assert_eq!(&pixel[..3], &[pixel[0]; 3], "neutral RGB must stay neutral");
            let alpha = f64::from(ramp[index]) * 255.0;
            assert!(
                (f64::from(pixel[3]) - alpha).abs() <= 0.5001,
                "alpha must remain linear"
            );
            if index > 0 {
                assert!(
                    rgba[index - 1][3] <= pixel[3],
                    "linear alpha must be monotonic"
                );
            }
        }
        // Dense inputs can differ by less than one output code, so allowed
        // transfer approximations may reverse adjacent quantized results.
        // Check monotonicity at steps wider than that conversion bound while
        // retaining the pointwise reference check for every dense sample.
        let mut previous = rgba[0][0];
        for step in 1..=32 {
            let index = ramp.partition_point(|&value| value < step as f32 / 32.0);
            let next = rgba[index][0];
            assert!(
                previous <= next,
                "{name}: the coarse transfer must be monotonic"
            );
            previous = next;
        }
        assert_eq!(&pixels[..4], &[0; 4]);
        assert_eq!(&pixels[pixels.len() - 4..], &[255; 4]);
        println!("color-transfer {name}: maximum-reference-error={maximum_error:.9}");
    }
    // Separate attachment formats may round an alpha quantization tie in
    // different directions. Each path must still satisfy the linear-alpha
    // bound above; record disagreement without requiring the same tie choice.
    let alpha_differences = srgb
        .as_chunks::<4>()
        .0
        .iter()
        .zip(explicit.as_chunks::<4>().0)
        .filter(|(a, b)| a[3] != b[3])
        .count();
    println!(
        "color-transfer differing alpha pixels: {alpha_differences}/{}",
        samples.len()
    );
    let differing = srgb
        .as_chunks::<4>()
        .0
        .iter()
        .zip(explicit.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    println!(
        "color-transfer differing pixels: {differing}/{}",
        samples.len()
    );
    for value in CONSTANTS {
        let index = ramp.iter().position(|&sample| sample == value).unwrap();
        println!(
            "color-transfer sample linear={value:.9} bits={:08x}: attachment={:?} explicit={:?}",
            value.to_bits(),
            &srgb[index * 4..index * 4 + 4],
            &explicit[index * 4..index * 4 + 4]
        );
    }
}
