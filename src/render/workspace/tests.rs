use super::*;

#[test]
fn shared_frame_preserves_revisions_and_rejects_invalid_replacement() {
    let capture = pollster::block_on(crate::doc_capture::Capture::new(640, 480)).unwrap();
    let device = &capture.device;
    let queue = &capture.queue;
    let size = [640, 480];
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (mut graphics, texture) = WorkspaceRenderer::new(device, format, size);
    let mut state = WorkspaceUi::new(texture);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/obj/cube-quads.obj");
    graphics
        .install(
            device,
            queue,
            &mut state,
            fixture.clone(),
            crate::asset_io::load(&fixture).unwrap(),
            false,
        )
        .unwrap();
    let document = state.editor.document.clone();
    let revision = state.mesh_revision;
    let mut invalid = crate::asset_io::load(&fixture).unwrap();
    invalid
        .document
        .objects
        .push(invalid.document.objects[0].clone());
    assert!(
        graphics
            .install(device, queue, &mut state, fixture, invalid, false)
            .is_err()
    );
    assert_eq!(state.editor.document, document);
    assert_eq!(state.mesh_revision, revision);
    assert_eq!(graphics.uploaded_revision, revision);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shared host renderer test"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let ctx = egui::Context::default();
    crate::workspace_ui::configure_context(&ctx);
    for selected in [false, true, false] {
        if selected {
            state.dispatch(crate::shortcuts::Command::SelectAll, &ctx, false);
        } else {
            state.editor.selected_objects.clear();
        }
        let previous_assets = state.asset_views.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0] as f32, size[1] as f32),
                )),
                ..Default::default()
            },
            |ui| state.ui(ui),
        );
        state.refresh_mesh().unwrap();
        graphics.paint(
            FrameTarget {
                device,
                queue,
                view: &view,
                size,
            },
            &ctx,
            &mut state,
            &mut output,
            previous_assets,
            &mut crate::measurement::FrameProbe::default(),
        );
        graphics.finish_frame(std::mem::take(&mut output.textures_delta.free));
        assert_eq!(
            state.mesh_revision, revision,
            "Selection does not rebuild authored geometry"
        );
        assert_eq!(graphics.uploaded_revision, revision);
        assert_eq!(state.editor.document, document);
        assert!(state.error.is_none(), "{:?}", state.error);
    }
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
}

#[cfg(feature = "viewport-measure")]
#[test]
fn scene_only_presentation_matches_egui_pixels_and_restores_texture_after_resize() {
    let capture = pollster::block_on(crate::doc_capture::Capture::new(640, 480)).unwrap();
    let device = &capture.device;
    let queue = &capture.queue;
    let size = [640, 480];
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/obj/cube-quads.obj");
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let (mut graphics, texture) = WorkspaceRenderer::new(device, format, size);
        let mut state = WorkspaceUi::new(texture);
        graphics
            .install(
                device,
                queue,
                &mut state,
                fixture.clone(),
                crate::asset_io::load(&fixture).unwrap(),
                false,
            )
            .unwrap();
        state.editor.selected_objects.clear();
        state.viewport =
            egui::Rect::from_min_size(egui::pos2(20.0, 16.0), egui::vec2(220.0, 164.0));
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene presentation equivalence test"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
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
        let frame_target = || FrameTarget {
            device,
            queue,
            view: &view,
            size,
        };
        let ctx = egui::Context::default();
        let mut probe = crate::measurement::FrameProbe::default();
        // A no-UI resize deliberately leaves egui's old texture registered.
        // Re-entering normal paint must rebind it even though size is now stable.
        for pixels_per_point in [1.0, 2.0] {
            graphics.paint_without_ui(
                frame_target(),
                &mut state,
                BTreeMap::new(),
                pixels_per_point,
                true,
                &mut probe,
            );
            assert!(graphics.scene_texture_dirty);
            assert_eq!(
                graphics.scene_size,
                [
                    (220.0 * pixels_per_point) as u32,
                    (164.0 * pixels_per_point) as u32
                ]
            );
            let without_ui = read_pixels(device, queue, &target);
            // The no-UI composite retains exactly the same scene rectangle.
            assert_eq!(&without_ui[..4], &[0, 0, 0, 255]);
            assert!(
                without_ui
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| *pixel != [0, 0, 0, 255])
            );

            ctx.set_pixels_per_point(pixels_per_point);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0] as f32, size[1] as f32) / pixels_per_point,
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.painter().image(
                        texture,
                        state.viewport,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                },
            );
            assert_eq!(output.pixels_per_point, pixels_per_point);
            graphics.paint(
                frame_target(),
                &ctx,
                &mut state,
                &mut output,
                BTreeMap::new(),
                &mut probe,
            );
            graphics.finish_frame(std::mem::take(&mut output.textures_delta.free));
            assert!(!graphics.scene_texture_dirty);
            let with_egui = read_pixels(device, queue, &target);
            assert!(
                without_ui == with_egui,
                "Scene composite pixels differ for {format:?} at {pixels_per_point} pixels/point"
            );

            // Renderer isolation must remove feedback left by an editor frame,
            // without mutating the editor's selection or rebuilding geometry.
            state.dispatch(crate::shortcuts::Command::SelectAll, &ctx, false);
            graphics.paint_without_ui(
                frame_target(),
                &mut state,
                BTreeMap::new(),
                pixels_per_point,
                true,
                &mut probe,
            );
            assert!(without_ui != read_pixels(device, queue, &target));
            let selected = state.editor.selected_objects.clone();
            let revision = state.mesh_revision;
            graphics.paint_without_ui(
                frame_target(),
                &mut state,
                BTreeMap::new(),
                pixels_per_point,
                false,
                &mut probe,
            );
            assert!(
                without_ui == read_pixels(device, queue, &target),
                "Renderer isolation retained selection feedback"
            );
            assert_eq!(state.editor.selected_objects, selected);
            assert_eq!(state.mesh_revision, revision);
            assert_eq!(graphics.uploaded_revision, revision);
            assert!(state.error.is_none(), "{:?}", state.error);
            state.editor.selected_objects.clear();
        }
    }
}

#[cfg(feature = "viewport-measure")]
fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let size = texture.size();
    let stride = (size.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene presentation equivalence readback"),
        size: u64::from(stride) * u64::from(size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(1))
        .unwrap()
        .unwrap();
    buffer
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..size.width as usize * 4].iter().copied())
        .collect()
}
