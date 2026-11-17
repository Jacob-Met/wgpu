use super::{InitTracker, MemoryInitKind};
use crate::resource::{Texture, TextureView};
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use arrayvec::ArrayVec;
use core::ops::Range;

#[derive(Debug, Clone)]
pub(crate) struct TextureInitRange {
    pub(crate) mip_range: Range<u32>,
    // Strictly array layers. We do *not* track volume slices separately.
    pub(crate) layer_range: Range<u32>,
    pub(crate) aspect: wgt::TextureAspect,
}

// Returns true if a copy operation doesn't fully cover the texture init
// tracking granularity. I.e. if this function returns true for a pending copy
// operation, the target texture needs to be ensured to be initialized first!
pub(crate) fn has_copy_partial_init_tracker_coverage<T>(
    copy_size: &wgt::Extent3d,
    copy_info: &wgt::TexelCopyTextureInfo<T>,
    desc: &wgt::TextureDescriptor<String, Vec<wgt::TextureFormat>>,
) -> bool {
    let mut target_size = desc.mip_level_size(copy_info.mip_level).unwrap();
    let is_planar = desc.format.is_multi_planar_format();
    if is_planar {
        let (w_sub, h_sub) = desc.format.subsampling_factors(copy_info.aspect.to_plane());
        target_size.width /= w_sub;
        target_size.height /= h_sub;
    }
    copy_size.width != target_size.width
        || copy_size.height != target_size.height
        || (desc.dimension == wgt::TextureDimension::D3
            && copy_size.depth_or_array_layers != target_size.depth_or_array_layers)
        || (!is_planar && copy_info.aspect != wgt::TextureAspect::All)
}

impl From<&'_ TextureView> for TextureInitRange {
    fn from(view: &'_ TextureView) -> Self {
        // TextureInitRange is always array layers, never depth slices.
        let layer_range = if view.parent.desc.dimension == wgt::TextureDimension::D3 {
            0..1
        } else {
            view.selector.layers.clone()
        };
        TextureInitRange {
            mip_range: view.selector.mips.clone(),
            layer_range,
            aspect: view.desc.range.aspect,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TextureInitTrackerAction {
    pub(crate) texture: Arc<Texture>,
    pub(crate) range: TextureInitRange,
    pub(crate) kind: MemoryInitKind,
}

pub(crate) type TextureLayerInitTracker = InitTracker<u32>;

#[derive(Debug)]
pub(crate) struct TextureInitTracker {
    mips: ArrayVec<TextureLayerInitTracker, { hal::MAX_MIP_LEVELS as usize }>,
    additional_planes:
        Option<Box<[ArrayVec<TextureLayerInitTracker, { hal::MAX_MIP_LEVELS as usize }>]>>,
}

impl TextureInitTracker {
    pub(crate) fn new(mip_level_count: u32, depth_or_array_layers: u32, plane_count: u32) -> Self {
        assert!(plane_count >= 1 && plane_count <= 3);
        let init_mips = || {
            core::iter::repeat_n(
                TextureLayerInitTracker::new(depth_or_array_layers),
                mip_level_count as usize,
            )
            .collect()
        };
        TextureInitTracker {
            mips: init_mips(),
            additional_planes: (plane_count > 1).then(|| {
                (1..plane_count)
                    .map(|_| init_mips())
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            }),
        }
    }

    fn plane_count(&self) -> usize {
        1 + self
            .additional_planes
            .as_ref()
            .map_or(0, |planes| planes.len())
    }

    fn plane_mips(
        &self,
        index: usize,
    ) -> &ArrayVec<TextureLayerInitTracker, { hal::MAX_MIP_LEVELS as usize }> {
        if index == 0 {
            &self.mips
        } else {
            &self.additional_planes.as_ref().unwrap()[index - 1]
        }
    }

    fn plane_mips_mut(
        &mut self,
        index: usize,
    ) -> &mut ArrayVec<TextureLayerInitTracker, { hal::MAX_MIP_LEVELS as usize }> {
        if index == 0 {
            &mut self.mips
        } else {
            &mut self.additional_planes.as_mut().unwrap()[index - 1]
        }
    }

    pub(crate) fn tracked_aspects(
        &self,
        aspect: wgt::TextureAspect,
    ) -> ArrayVec<wgt::TextureAspect, 3> {
        let mut aspects = ArrayVec::new();
        if self.plane_count() == 1 {
            aspects.push(aspect);
        } else if aspect == wgt::TextureAspect::All {
            for plane in 0..self.plane_count() {
                aspects.push(wgt::TextureAspect::from_plane(plane as u32).unwrap());
            }
        } else {
            assert!(
                aspect
                    .to_plane()
                    .is_some_and(|plane| (plane as usize) < self.plane_count()),
                "invalid planar texture aspect {aspect:?}"
            );
            aspects.push(aspect);
        }
        aspects
    }

    fn plane_index(&self, aspect: wgt::TextureAspect) -> usize {
        if self.plane_count() == 1 {
            0
        } else {
            aspect
                .to_plane()
                .expect("planar init requires a plane aspect") as usize
        }
    }

    pub(crate) fn mip(
        &self,
        mip_level: u32,
        aspect: wgt::TextureAspect,
    ) -> &TextureLayerInitTracker {
        &self.plane_mips(self.plane_index(aspect))[mip_level as usize]
    }

    pub(crate) fn mip_mut(
        &mut self,
        mip_level: u32,
        aspect: wgt::TextureAspect,
    ) -> &mut TextureLayerInitTracker {
        let index = self.plane_index(aspect);
        &mut self.plane_mips_mut(index)[mip_level as usize]
    }

    pub(crate) fn mark_externally_initialized(&mut self, layers: Range<u32>) {
        let mark =
            |plane: &mut ArrayVec<TextureLayerInitTracker, { hal::MAX_MIP_LEVELS as usize }>| {
                for mip in plane.iter_mut() {
                    mip.drain(layers.clone());
                }
            };
        mark(&mut self.mips);
        if let Some(planes) = &mut self.additional_planes {
            for plane in planes.iter_mut() {
                mark(plane);
            }
        }
    }

    pub(crate) fn drain(
        &mut self,
        range: &TextureInitRange,
    ) -> Vec<(wgt::TextureAspect, u32, Range<u32>)> {
        let mut pending = Vec::new();
        for aspect in self.tracked_aspects(range.aspect) {
            let plane = self.plane_index(aspect);
            for mip in range.mip_range.clone() {
                for layers in
                    self.plane_mips_mut(plane)[mip as usize].drain(range.layer_range.clone())
                {
                    pending.push((aspect, mip, layers));
                }
            }
        }
        pending
    }

    pub(crate) fn check_action(
        &self,
        action: &TextureInitTrackerAction,
    ) -> Vec<TextureInitTrackerAction> {
        let mut actions = Vec::new();
        for aspect in self.tracked_aspects(action.range.aspect) {
            let plane = self.plane_index(aspect);
            let mut mip_range_start = usize::MAX;
            let mut mip_range_end = usize::MIN;
            let mut layer_range_start = u32::MAX;
            let mut layer_range_end = u32::MIN;

            for (i, mip_tracker) in self
                .plane_mips(plane)
                .iter()
                .enumerate()
                .take(action.range.mip_range.end as usize)
                .skip(action.range.mip_range.start as usize)
            {
                if let Some(uninitialized_layer_range) =
                    mip_tracker.check(action.range.layer_range.clone())
                {
                    mip_range_start = mip_range_start.min(i);
                    mip_range_end = i + 1;
                    layer_range_start = layer_range_start.min(uninitialized_layer_range.start);
                    layer_range_end = layer_range_end.max(uninitialized_layer_range.end);
                }
            }

            if mip_range_start < mip_range_end && layer_range_start < layer_range_end {
                actions.push(TextureInitTrackerAction {
                    texture: action.texture.clone(),
                    range: TextureInitRange {
                        mip_range: mip_range_start as u32..mip_range_end as u32,
                        layer_range: layer_range_start..layer_range_end,
                        aspect,
                    },
                    kind: action.kind,
                });
            }
        }
        actions
    }

    pub(crate) fn discard(&mut self, mip_level: u32, layer: u32, aspect: wgt::TextureAspect) {
        for aspect in self.tracked_aspects(aspect) {
            self.mip_mut(mip_level, aspect).discard(layer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextureInitTracker;
    use wgt::TextureAspect;

    #[test]
    fn full_plane_copy_does_not_require_prior_zeroing() {
        let desc = wgt::TextureDescriptor {
            label: alloc::string::String::new(),
            size: wgt::Extent3d {
                width: 256,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::NV12,
            usage: wgt::TextureUsages::COPY_DST,
            view_formats: alloc::vec::Vec::new(),
            texture_binding_view_dimension: None,
        };
        for (aspect, size) in [
            (TextureAspect::Plane0, desc.size),
            (
                TextureAspect::Plane1,
                wgt::Extent3d {
                    width: 128,
                    height: 64,
                    depth_or_array_layers: 1,
                },
            ),
        ] {
            let info = wgt::TexelCopyTextureInfo {
                texture: (),
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect,
            };
            assert!(!super::has_copy_partial_init_tracker_coverage(
                &size, &info, &desc
            ));
            let partial = wgt::Extent3d {
                width: size.width - 2,
                ..size
            };
            assert!(super::has_copy_partial_init_tracker_coverage(
                &partial, &info, &desc
            ));
        }
    }

    #[test]
    fn planar_discard_preserves_untouched_plane() {
        let mut tracker = TextureInitTracker::new(1, 1, 2);
        for aspect in [TextureAspect::Plane0, TextureAspect::Plane1] {
            tracker.mip_mut(0, aspect).drain(0..1);
        }
        tracker.discard(0, 0, TextureAspect::Plane0);
        assert!(tracker.mip(0, TextureAspect::Plane0).check(0..1).is_some());
        assert!(tracker.mip(0, TextureAspect::Plane1).check(0..1).is_none());
    }

    #[test]
    fn planar_init_tracks_write_and_discard_per_plane() {
        let mut tracker = TextureInitTracker::new(1, 2, 2);
        tracker.mip_mut(0, TextureAspect::Plane1).drain(0..1);
        assert!(tracker.mip(0, TextureAspect::Plane1).check(0..1).is_none());
        assert!(tracker.mip(0, TextureAspect::Plane1).check(1..2).is_some());
        assert!(tracker.mip(0, TextureAspect::Plane0).check(0..1).is_some());
        tracker.mip_mut(0, TextureAspect::Plane0).drain(0..2);
        tracker.discard(0, 1, TextureAspect::All);
        assert!(tracker.mip(0, TextureAspect::Plane0).check(1..2).is_some());
        assert!(tracker.mip(0, TextureAspect::Plane1).check(1..2).is_some());
        assert!(tracker.mip(0, TextureAspect::Plane0).check(0..1).is_none());
        assert!(tracker.mip(0, TextureAspect::Plane1).check(0..1).is_none());
    }
}
