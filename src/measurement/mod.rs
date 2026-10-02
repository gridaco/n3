//! Opt-in viewport diagnostics. Normal builds erase probes at this module boundary.
#[cfg(feature = "viewport-measure")]
mod enabled;
#[cfg(feature = "viewport-measure")]
pub(crate) use enabled::*;
#[cfg(not(feature = "viewport-measure"))]
mod disabled;
#[cfg(not(feature = "viewport-measure"))]
pub(crate) use disabled::*;

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Acquire,
    Ui,
    Commands,
    HostPrepare,
    CacheSync,
    Tessellate,
    Feedback,
    SceneEncode,
    UiEncode,
    Submit,
    Present,
    HostTail,
    #[cfg(feature = "viewport-measure")]
    EditorPrepare,
    #[cfg(feature = "viewport-measure")]
    SceneComposite,
}
