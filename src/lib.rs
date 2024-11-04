use std::io::{Read as _, Seek as _};

pub mod ass;
pub mod cli;
pub mod japanese;
pub mod srt;
pub mod utils;
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

/// Support subtitle formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubtitleFormat {
    Srt,
    Ass,
    Vtt,
}

impl SubtitleFormat {
    /// Detects the subtitle format from the string buffer.
    pub fn detect(s: &str) -> Option<Self> {
        if s.starts_with("[Script Info]") {
            Some(Self::Ass)
        } else if s.starts_with("WEBVTT") {
            Some(Self::Vtt)
        } else if s.starts_with('1') {
            Some(Self::Srt)
        } else {
            None
        }
    }
}
