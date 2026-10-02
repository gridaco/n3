//! Each target keeps its monotonic clock, including browser Performance timing.
#[cfg(not(target_arch = "wasm32"))]
pub(super) use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub(super) use web_time::Instant;
