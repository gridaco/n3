//! glTF 2 import adapter. Source syntax, buffers, and meters end here.
mod accessors;
mod budgets;
mod decode;
mod geometry;
mod materials;
mod resources;
mod units;

use crate::asset_io::ResourceResolver;
use crate::scene::*;
pub(crate) use decode::load;
use glam::{DMat4, DQuat, DVec3};

#[cfg(test)]
mod fingerprint_tests;
#[cfg(test)]
mod tests;
