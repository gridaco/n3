//! Native entry point for the shared N3 application.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    n3::run_native()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
