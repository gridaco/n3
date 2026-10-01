//! Runtime scene allocation and per-frame deformation limits.
use super::*;

pub(crate) const MAX_PRIMITIVES: usize = 4096;
pub(crate) const MAX_OWNED_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_FRAME_VERTICES: usize = 4_000_000;
pub(crate) const MAX_FRAME_INDICES: usize = 12_000_000;
pub(crate) const MAX_DEFORMATION_WORK: usize = 32_000_000;

#[derive(Default)]
pub(crate) struct Budget {
    used: usize,
}
impl Budget {
    pub(crate) fn charge(&mut self, count: usize, size: usize, maximum: usize) -> Result<()> {
        self.used = count
            .checked_mul(size)
            .and_then(|bytes| self.used.checked_add(bytes))
            .filter(|total| *total <= maximum)
            .ok_or("Scene exceeds its decoded payload or evaluation work budget.")?;
        Ok(())
    }

    pub(crate) fn owned<T>(&mut self, count: usize) -> Result<()> {
        self.charge(count, std::mem::size_of::<T>(), MAX_OWNED_BYTES)
    }
}

/// Charge all active instances before deformation starts. Morph and joint work
/// includes zero weights, so playback cannot cross a budget boundary mid-clip.
pub(crate) fn evaluated(asset: &SceneAsset, active: &[bool]) -> Result<()> {
    let mut vertices = Budget::default();
    let mut work = Budget::default();
    let mut draws = 0;
    let mut lights = 0;
    for node in asset
        .nodes
        .iter()
        .enumerate()
        .filter(|(id, _)| active[*id])
        .map(|(_, node)| node)
    {
        lights += usize::from(node.light.is_some());
        if lights > MAX_ACTIVE_LIGHTS {
            return Err(format!(
                "Scene exceeds the {MAX_ACTIVE_LIGHTS} active punctual light budget."
            ));
        }
        let Some(mesh) = node.mesh else { continue };
        for primitive in &asset.meshes[mesh].primitives {
            draws += 1;
            if draws > MAX_PRIMITIVES {
                return Err("Scene instances exceed the draw count budget.".into());
            }
            let count = if primitive.flat_normals && primitive.topology == Topology::Triangles {
                primitive.indices.len()
            } else {
                primitive.vertices.len()
            };
            vertices.charge(count, 1, MAX_FRAME_VERTICES)?;
            let morphs = primitive
                .morphs
                .iter()
                .map(|morph| {
                    usize::from(!morph.positions.is_empty())
                        + usize::from(!morph.normals.is_empty())
                        + usize::from(!morph.tangents.is_empty())
                })
                .sum::<usize>();
            let influences = if node.skin.is_some() {
                primitive
                    .influences
                    .first()
                    .map_or(0, |sets| sets.len() * 4)
            } else {
                0
            };
            work.charge(
                primitive.vertices.len(),
                1 + morphs + influences,
                MAX_DEFORMATION_WORK,
            )?;
        }
    }
    Ok(())
}

/// Bound retained distinct source-scene previews, including those held for Undo.
/// Evaluating each source safely is insufficient if a document retains many poses.
pub(crate) fn retained_frames<'a>(
    frames: impl IntoIterator<Item = &'a EvaluatedScene>,
) -> Result<()> {
    let mut vertices = Budget::default();
    let mut indices = Budget::default();
    for frame in frames {
        for draw in &frame.draws {
            vertices.charge(draw.vertices.len(), 1, MAX_FRAME_VERTICES)?;
            indices.charge(draw.indices.len(), 1, MAX_FRAME_INDICES)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod retained_tests {
    use super::*;

    #[test]
    fn individually_small_scenes_cannot_accumulate_unbounded_preview_payloads() {
        let asset = SceneAsset::new(crate::scene::test_support::triangle_data()).unwrap();
        let mut frame = (*asset.evaluate(0, None).unwrap()).clone();
        let vertex = frame.draws[0].vertices[0];
        frame.draws[0].vertices.resize(1000, vertex);
        // Count tiny shared test data as distinct retained previews without
        // allocating the millions of vertices that this guard prevents.
        let allowed = MAX_FRAME_VERTICES / 1000;
        retained_frames(std::iter::repeat_n(&frame, allowed)).unwrap();
        assert!(retained_frames(std::iter::repeat_n(&frame, allowed + 1)).is_err());
        frame.draws[0].vertices.clear();
        frame.draws[0].indices = vec![0; 1000].into();
        let allowed = MAX_FRAME_INDICES / 1000;
        retained_frames(std::iter::repeat_n(&frame, allowed)).unwrap();
        assert!(retained_frames(std::iter::repeat_n(&frame, allowed + 1)).is_err());
    }
}
