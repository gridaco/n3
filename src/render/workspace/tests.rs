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
