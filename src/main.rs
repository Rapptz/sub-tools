use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use sub_tools::srt::Dialogue;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The subtitle files to operate on
    files: Vec<PathBuf>,
    /// Shift the times of the subtitles by the given seconds
    #[arg(long, value_parser = valid_duration, allow_negative_numbers = true)]
    shift: f32,
}

fn valid_duration(s: &str) -> Result<f32, String> {
    let time: f32 = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a valid duration"))?;
    if !time.is_finite() || time == -0.0 {
        Err(format!("`{s}` isn't a valid duration"))
    } else {
        Ok(time)
    }
}

fn shift_subtitle_file(file: PathBuf, shift: f32) -> anyhow::Result<()> {
    let mut fp = File::open(&file)?;
    let new_filename = match file.file_stem() {
        Some(filename) => {
            let mut new_file = PathBuf::new();
            let mut filename = filename.to_os_string();
            filename.push("_shifted.srt");
            new_file.set_file_name(filename);
            new_file
        }
        None => anyhow::bail!("invalid filename given"),
    };

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

    let mut dialogue = buffer
        .split_terminator("\n\n")
        .enumerate()
        .map(|(i, s)| {
            s.parse::<Dialogue>()
                .with_context(|| format!("from srt dialogue {}", i + 1))
        })
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to extract dialogue from {}", file.display()))?;

    let mut new_contents = dialogue
        .iter_mut()
        .map(|f| {
            f.shift_by(shift);
            f.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    new_contents.push_str("\n\n");
    let mut new_fp = File::create(new_filename)
        .with_context(|| "could not create new subtitle file".to_string())?;
    new_fp.write_all(new_contents.as_bytes())?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    for file in args.files {
        shift_subtitle_file(file, args.shift)?;
    }
    Ok(())
}
