//! Regenerates `docs/supported-devices.md` from the device database.
//!
//! Run from the repository root: `cargo run -p device-db --bin gen-supported-devices`.
//! A test in this crate fails when the checked-in document has drifted, so the
//! table and the code cannot disagree.

fn main() -> std::io::Result<()> {
    let path = std::path::Path::new("docs/supported-devices.md");
    std::fs::write(path, device_db::render_supported_devices())?;
    println!("wrote {}", path.display());
    Ok(())
}
