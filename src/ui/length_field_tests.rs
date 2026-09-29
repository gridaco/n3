use super::length_field;
use crate::units::{LengthUnit, format_length_number};

struct FieldFrame {
    field: egui::Rect,
    other: egui::Rect,
    field_id: egui::Id,
    changed: bool,
    text: Vec<String>,
}

fn frame(
    ctx: &egui::Context,
    centimeters: &mut f64,
    unit: LengthUnit,
    events: Vec<egui::Event>,
) -> FieldFrame {
    let mut result = None;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 180.0),
            )),
            events,
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                let field = length_field(ui, centimeters, 0.02, unit);
                let other = ui.button("Other control");
                if other.clicked() {
                    other.request_focus();
                }
                result = Some(FieldFrame {
                    field: field.rect,
                    other: other.rect,
                    field_id: field.id,
                    changed: field.changed(),
                    text: Vec::new(),
                });
            });
        },
    );
    output.textures_delta.clear();
    let mut result = result.unwrap();
    result.text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    result
}

fn pointer(position: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn focusing_and_blurring_length_fields_preserves_tiny_and_precise_canonical_values() {
    for unit in LengthUnit::ALL {
        for original in [
            1.0e-200_f64,
            1.0e-12,
            0.123_456_789_012_345_66,
            -12.345_678_901_234_567,
        ] {
            let ctx = egui::Context::default();
            let mut centimeters = original;
            frame(&ctx, &mut centimeters, unit, vec![]);
            let idle = frame(&ctx, &mut centimeters, unit, vec![]);
            let number = format_length_number(unit.from_centimeters(original));
            assert_ne!(number.parse::<f64>().unwrap(), 0.0);
            assert!(
                idle.text.contains(&number),
                "Actual field must show bare {number} in {unit:?}, got {:?}",
                idle.text
            );
            assert!(!idle.text.contains(&format!("{number} {}", unit.symbol())));
            let position = idle.field.center();
            for events in [
                vec![egui::Event::PointerMoved(position), pointer(position, true)],
                vec![pointer(position, false)],
                vec![],
            ] {
                let response = frame(&ctx, &mut centimeters, unit, events);
                assert!(!response.changed);
                assert_eq!(centimeters.to_bits(), original.to_bits());
            }
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(idle.field_id));
            let focused = frame(&ctx, &mut centimeters, unit, vec![]);
            assert!(focused.text.contains(&number));
            let position = focused.other.center();
            for events in [
                vec![egui::Event::PointerMoved(position), pointer(position, true)],
                vec![pointer(position, false)],
                vec![],
                vec![],
            ] {
                let response = frame(&ctx, &mut centimeters, unit, events);
                assert!(
                    !response.changed,
                    "An untouched {original} cm field changed on blur in {unit:?}"
                );
                assert_eq!(centimeters.to_bits(), original.to_bits());
            }
            assert_ne!(ctx.memory(|memory| memory.focused()), Some(idle.field_id));
        }
    }
}

#[test]
fn changing_display_units_reformats_without_roundtripping_canonical_drafts() {
    let ctx = egui::Context::default();
    let original = 0.123_456_789_012_345_66_f64;
    let mut centimeters = original;
    for unit in LengthUnit::ALL {
        let response = frame(&ctx, &mut centimeters, unit, vec![]);
        assert!(!response.changed);
        assert_eq!(centimeters.to_bits(), original.to_bits());
        let number = format_length_number(unit.from_centimeters(original));
        assert!(response.text.contains(&number));
        assert!(
            !response
                .text
                .contains(&format!("{number} {}", unit.symbol()))
        );
    }
}
