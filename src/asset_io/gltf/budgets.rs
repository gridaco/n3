//! Preflight retained allocations before decoding format accessors.
use super::*;
use crate::scene::budgets::{Budget, MAX_PRIMITIVES};

/// Preflight before allocating copied arrays. Temporary accessor decoding has a
/// separate bound; this caps aggregate retained geometry/skin/animation data.
pub(super) fn decoded(document: &::gltf::Document) -> Result<()> {
    let mut budget = Budget::default();
    let mut primitive_count = 0;
    let mut vertex_count = Budget::default();
    let mut index_count = Budget::default();
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            primitive_count += 1;
            if primitive_count > MAX_PRIMITIVES {
                return Err("Scene exceeds the primitive count budget.".into());
            }
            let count = primitive
                .get(&::gltf::Semantic::Positions)
                .ok_or("A mesh primitive is missing POSITION.")?
                .count();
            vertex_count.charge(count, 1, 2_000_000)?;
            budget.owned::<Primitive>(1)?;
            budget.owned::<SceneVertex>(count)?;
            let indices = primitive
                .indices()
                .map_or(count, |accessor| accessor.count());
            let (_, expanded) = geometry::topology_count(primitive.mode(), indices)?;
            index_count.charge(expanded, 1, geometry::MAX_INDICES)?;
            budget.owned::<u32>(expanded)?;
            let mut targets = 0;
            for target in primitive.morph_targets() {
                targets += 1;
                if targets > 64 {
                    return Err("A primitive supports at most 64 morph targets.".into());
                }
                budget.owned::<Morph>(1)?;
                if let Some(accessor) = target.positions() {
                    budget.owned::<[f64; 3]>(accessor.count())?;
                }
                for accessor in [target.normals(), target.tangents()].into_iter().flatten() {
                    budget.owned::<[f32; 3]>(accessor.count())?;
                }
            }
            let sets = primitive
                .attributes()
                .filter(|(semantic, _)| matches!(semantic, ::gltf::Semantic::Joints(_)))
                .count();
            if sets > 8 {
                return Err("A primitive supports at most eight joint influence sets.".into());
            }
            if sets != 0 {
                budget.owned::<Vec<([u16; 4], [f32; 4])>>(count)?;
                budget.owned::<([u16; 4], [f32; 4])>(
                    count.checked_mul(sets).ok_or("Influence count overflow.")?,
                )?;
            }
        }
    }
    for skin in document.skins() {
        budget.owned::<DMat4>(skin.joints().len())?;
        budget.owned::<usize>(skin.joints().len())?;
    }
    for animation in document.animations() {
        if animation.channels().count() > 16_384 {
            return Err("Animation exceeds the channel count budget.".into());
        }
        for channel in animation.channels() {
            budget.owned::<Channel>(1)?;
            for accessor in [channel.sampler().input(), channel.sampler().output()] {
                let values = accessor
                    .count()
                    .checked_mul(accessor.dimensions().multiplicity())
                    .ok_or("Animation count overflow.")?;
                budget.owned::<f64>(values)?;
            }
        }
    }
    Ok(())
}
