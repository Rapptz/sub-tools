use std::io::{Read as _, Seek as _};

pub mod japanese;
pub mod srt;
pub mod vtt;

/// Loads a file into a string.
///
/// This checks for the UTF-8 BOM and strips it
pub(crate) fn load_file(path: &std::path::Path) -> std::io::Result<String> {
    let mut fp = std::fs::File::open(path)?;

    let mut buffer = String::new();
    // Try to check if there's a UTF-8 BOM somewhere
    let mut bom: [u8; 3] = [0; 3];
    fp.read_exact(&mut bom)?;
    if bom != [0xEF, 0xBB, 0xBF] {
        fp.rewind()?;
    }

    fp.read_to_string(&mut buffer)?;

    if buffer.contains("\r\n") {
        buffer = buffer.replace("\r\n", "\n");
    }

    Ok(buffer)
}
