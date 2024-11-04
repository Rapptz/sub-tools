//! A parser for SubStation Alpha v4+ (.ass subtitles)
//!
//! The parser has to have a few properties, mainly because it's used for
//! simple modifications.
//!
//! 1. It should preserve comments that are given
//! 2. The output must be mostly identical to the pre-existing file as much as possible

use std::fmt::{Display, Write};
use std::io::BufRead;
use std::path::Path;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;

/// An error that occurs during parsing
#[derive(Debug)]
pub enum ErrorKind {
    /// An I/O error occurred.
    Io(std::io::Error),
    /// The file is invalid.
    Invalid,
    /// The file is missing the `[Script Info]` header.
    MissingScriptInfo,
    /// The file is missing a style format
    MissingStyleFormat,
    /// There is an invalid style in the file
    InvalidStyle,
    /// The event type is invalid
    InvalidEventType,
    /// There is an invalid event in the file
    InvalidEvent,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    line: usize,
}

impl Error {
    fn from_kind(kind: ErrorKind) -> Self {
        Self { kind, line: 0 }
    }

    fn with_line(self, line: usize) -> Self {
        Self {
            kind: self.kind,
            line,
        }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn line(&self) -> usize {
        self.line
    }
}

impl From<std::io::Error> for Error {
    fn from(v: std::io::Error) -> Self {
        Self::from_kind(ErrorKind::Io(v))
    }
}

impl From<ErrorKind> for Error {
    fn from(value: ErrorKind) -> Self {
        Self::from_kind(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind() {
            ErrorKind::Io(error) => write!(f, "line {}: file error: {error}", self.line),
            ErrorKind::Invalid => write!(f, "line {}: .ass file is invalid", self.line),
            ErrorKind::MissingScriptInfo => {
                write!(f, "line {}: missing [Script Info] header", self.line)
            }
            ErrorKind::MissingStyleFormat => {
                write!(f, "line {}: missing format for styles", self.line)
            }
            ErrorKind::InvalidStyle => write!(f, "line {}: style is invalid", self.line),
            ErrorKind::InvalidEventType => write!(f, "line {}: event type is invalid", self.line),
            ErrorKind::InvalidEvent => write!(f, "line {}: event is invalid", self.line),
        }
    }
}

impl std::error::Error for Error {}

pub trait ToAss {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()>;
}

/// A line in an .ass file
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Line {
    /// Represents a key and a value pairing
    Variable(String),
    /// Represents a comment
    Comment(String),
    /// Represents an embedded UUEncoding line
    Encoded(String),
    /// Represents an empty line
    Empty,
}

impl Line {
    /// Returns the line as a key-value pair.
    ///
    /// Comments are returned as `None`.
    pub fn item(&self) -> Option<(&str, &str)> {
        match self {
            Line::Variable(v) => v.split_once(": "),
            _ => None,
        }
    }

    pub fn variable(key: &str, value: &str) -> Self {
        Self::Variable(format!("{key}: {value}"))
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            Some(Self::Empty)
        } else if let Some(suffix) = s.strip_prefix(';') {
            Some(Self::Comment(suffix.to_owned()))
        } else if s.split_once(": ").is_some() {
            Some(Self::Variable(s.to_owned()))
        } else if s.len() <= 80 && s.as_bytes().iter().all(|s| (33..97).contains(s)) {
            Some(Self::Encoded(s.to_owned()))
        } else {
            None
        }
    }

    /// Returns `true` if the line is a comment line.
    ///
    /// For key-value pairings, the `Comment:` key is considered a comment.
    pub fn is_comment(&self) -> bool {
        match self {
            Line::Variable(v) => v
                .split_once(": ")
                .map(|(key, _)| key == "Comment")
                .unwrap_or_default(),
            Line::Comment(_) => true,
            Line::Encoded(_) => false,
            Line::Empty => false,
        }
    }

    /// Overwrite the current line with the given key-value pair.
    pub fn overwrite(&mut self, key: &str, value: impl Display) {
        *self = Self::Variable(format!("{key}: {value}"))
    }

    /// Edit the current value in a key-value pair, if provided, with the given value.
    pub fn set(&mut self, value: impl Display) {
        if let Line::Variable(v) = self {
            if let Some(index) = v.find(": ") {
                v.truncate(index + 2);
                let _ = v.write_fmt(format_args!("{value}"));
            }
        }
    }

    /// Returns `true` if the line is [`Empty`].
    ///
    /// [`Empty`]: Line::Empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns `true` if the line is [`Encoded`].
    ///
    /// [`Encoded`]: Line::Encoded
    #[must_use]
    pub fn is_encoded(&self) -> bool {
        matches!(self, Self::Encoded(..))
    }
}

impl ToAss for Line {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Line::Variable(s) => writeln!(writer, "{s}"),
            Line::Comment(c) => writeln!(writer, ";{c}"),
            Line::Encoded(e) => writeln!(writer, "{e}"),
            Line::Empty => writeln!(writer),
        }
    }
}

trait SectionParse {
    fn process_line(&mut self, line: Line) -> Result<(), Error>;
}

/// The script info of the .ass file.
///
/// This is the information that belongs in the `[Script Info]` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInfo {
    lines: Vec<Line>,
}

impl Default for ScriptInfo {
    fn default() -> Self {
        Self {
            lines: vec![
                Line::variable("ScriptType", "v4.00+"),
                Line::variable("WrapStyle", "0"),
                Line::variable("ScaledBorderAndShadow", "yes"),
                Line::variable("YCbCr Matrix", "TV.709"),
                Line::variable("PlayResX", "1920"),
                Line::variable("PlayResY", "1080"),
                Line::Empty,
            ],
        }
    }
}

impl SectionParse for ScriptInfo {
    fn process_line(&mut self, line: Line) -> Result<(), Error> {
        if line.is_encoded() {
            Err(ErrorKind::Invalid.into())
        } else {
            self.lines.push(line);
            Ok(())
        }
    }
}

impl ToAss for ScriptInfo {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "[Script Info]")?;
        for line in &self.lines {
            line.to_ass(writer)?;
        }
        Ok(())
    }
}

impl ScriptInfo {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Returns the title of the script
    pub fn title(&self) -> &str {
        self.lines
            .iter()
            .filter_map(|l| l.item())
            .find_map(|(key, value)| (key == "Title").then_some(value))
            .unwrap_or("<untitled>")
    }

    /// Returns the version of the script
    pub fn version(&self) -> &str {
        self.lines
            .iter()
            .filter_map(|l| l.item())
            .find_map(|(key, value)| (key == "ScriptType").then_some(value))
            .unwrap_or_default()
    }

    pub fn remove_comments(&mut self) {
        self.lines.retain(|s| !s.is_comment());
    }
}

/// A generic section in the .ass file.
///
/// This is one that doesn't have any dedicated parseable information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericSection {
    lines: Vec<Line>,
    title: String,
}

impl SectionParse for GenericSection {
    fn process_line(&mut self, line: Line) -> Result<(), Error> {
        self.lines.push(line);
        Ok(())
    }
}

impl ToAss for GenericSection {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "[{}]", self.title)?;
        for line in &self.lines {
            line.to_ass(writer)?;
        }
        Ok(())
    }
}

impl GenericSection {
    fn new(title: &str) -> Self {
        Self {
            lines: Vec::new(),
            title: title.to_owned(),
        }
    }

    pub fn remove_comments(&mut self) {
        self.lines.retain(|s| !s.is_comment());
    }
}

/// Colour that is used in a style or .ass script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Colour {
    pub const WHITE: Colour = Colour::from_rgb(255, 255, 255);
    pub const BLACK: Colour = Colour::from_rgb(0, 0, 0);
    pub const RED: Colour = Colour::from_rgb(255, 0, 0);

    pub const fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 0,
        }
    }

    pub const fn from_rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn from_ass(s: &str) -> Option<Self> {
        // AABBGGRR
        let rest = s.strip_prefix("&H")?;
        let num = u32::from_str_radix(rest, 16).ok()?;
        Some(Self {
            red: (num & 0xFF) as u8,
            green: ((num >> 8) & 0xFF) as u8,
            blue: ((num >> 16) & 0xFF) as u8,
            alpha: ((num >> 24) & 0xFF) as u8,
        })
    }

    pub fn to_hex(&self) -> String {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            self.red, self.green, self.blue, self.alpha
        )
    }

    pub fn relative_luminance(&self) -> f32 {
        // Source: https://www.w3.org/TR/WCAG20/#relativeluminancedef
        let r = self.red as f32 / 255.0;
        let g = self.green as f32 / 255.0;
        let b = self.blue as f32 / 255.0;

        #[rustfmt::skip]
        let (r, g, b) = {
            let r = if r <= 0.03928 { r / 12.92 } else { ((r + 0.055) / 1.055).powf(2.4) };
            let g = if g <= 0.03928 { g / 12.92 } else { ((g + 0.055) / 1.055).powf(2.4) };
            let b = if b <= 0.03928 { b / 12.92 } else { ((b + 0.055) / 1.055).powf(2.4) };
            (r, g, b)
        };

        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}

impl Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "&H{a:02X}{b:02X}{g:02X}{r:02X}",
            a = self.alpha,
            b = self.blue,
            g = self.green,
            r = self.red
        )
    }
}

/// A style for the .ass script
#[derive(Debug, Clone)]
pub struct Style {
    pub name: String,
    pub font_name: String,
    pub font_size: u8,
    pub primary_colour: Colour,
    pub secondary_colour: Colour,
    pub outline_colour: Colour,
    pub background_colour: Colour,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub striked: bool,
    pub scale_x: f32,
    pub scale_y: f32,
    pub spacing: f32,
    pub angle: f32,
    pub border_style: u8, // Can be an enum
    pub outline: f32,
    pub shadow: f32,
    pub alignment: u8, // Can be an enum
    pub margin_l: u16,
    pub margin_r: u16,
    pub margin_v: u16,
    pub encoding: u8, // No idea about this one
}

impl Default for Style {
    fn default() -> Self {
        Self {
            name: String::from("Default"),
            font_name: String::from("Arial"),
            font_size: 20,
            primary_colour: Colour::WHITE,
            secondary_colour: Colour::RED,
            outline_colour: Colour::BLACK,
            background_colour: Colour::BLACK,
            bold: false,
            italic: false,
            underline: false,
            striked: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            border_style: 1,
            outline: 2.0,
            shadow: 2.0,
            alignment: 2,
            margin_l: 10,
            margin_r: 10,
            margin_v: 10,
            encoding: 1,
        }
    }
}

impl Style {
    /// The default style used for the sub-tools conversion scheme.
    ///
    /// The font should be Yu Gothic UI + bold if Japanese is found
    /// but that only really works on Windows so it might not be desirable
    /// to use that for general usage.
    fn program_default() -> Self {
        Self {
            name: String::from("Default"),
            font_name: String::from("Arial"),
            font_size: 66,
            primary_colour: Colour::from_rgb(0xFA, 0xFA, 0xFA),
            secondary_colour: Colour::RED,
            outline_colour: Colour::from_rgb(0xB6, 0x73, 0xF2),
            background_colour: Colour::BLACK,
            bold: false,
            italic: false,
            underline: false,
            striked: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 1.0,
            angle: 0.0,
            border_style: 1,
            outline: 3.0,
            shadow: 0.0,
            alignment: 2,
            margin_l: 10,
            margin_r: 10,
            margin_v: 20,
            encoding: 1,
        }
    }
}

impl ToAss for Style {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(
            writer,
            "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.name,
            self.font_name,
            self.font_size,
            self.primary_colour,
            self.secondary_colour,
            self.outline_colour,
            self.background_colour,
            if self.bold { 1 } else { 0 },
            if self.italic { 1 } else { 0 },
            if self.underline { 1 } else { 0 },
            if self.striked { 1 } else { 0 },
            self.scale_x,
            self.scale_y,
            self.spacing,
            self.angle,
            self.border_style,
            self.outline,
            self.shadow,
            self.alignment,
            self.margin_l,
            self.margin_r,
            self.margin_v,
            self.encoding,
        )
    }
}

/// The section that denotes the styles in the script
#[derive(Debug, Clone)]
pub struct StylesSection {
    format: Vec<String>,
    pub styles: Vec<Style>,
}

impl Default for StylesSection {
    fn default() -> Self {
        const DEFAULT_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
        Self {
            format: DEFAULT_FORMAT.split(", ").map(String::from).collect(),
            styles: vec![Style::default()],
        }
    }
}

impl StylesSection {
    fn new() -> Self {
        Self {
            format: Vec::new(),
            styles: Vec::new(),
        }
    }

    fn style_from_format(&self, data: &str) -> Option<Style> {
        let mut style = Style::default();
        // Technically, I don't think the order of these will ever change... but just for the sake of being "correct"
        // I should handle the `Format` being given as-is, even if it makes this code at least 10x more complicated.
        for (name, value) in self.format.iter().map(String::as_str).zip(data.split(',')) {
            match name {
                "Name" => style.name = value.to_owned(),
                "Fontname" => style.font_name = value.to_owned(),
                "Fontsize" => style.font_size = value.parse().ok()?,
                "PrimaryColour" => style.primary_colour = Colour::from_ass(value)?,
                "SecondaryColour" => style.secondary_colour = Colour::from_ass(value)?,
                "OutlineColour" => style.outline_colour = Colour::from_ass(value)?,
                "BackColour" => style.background_colour = Colour::from_ass(value)?,
                "Bold" => style.bold = value != "0",
                "Italic" => style.italic = value != "0",
                "Underline" => style.underline = value != "0",
                "StrikeOut" => style.striked = value != "0",
                "ScaleX" => style.scale_x = value.parse().ok()?,
                "ScaleY" => style.scale_y = value.parse().ok()?,
                "Spacing" => style.spacing = value.parse().ok()?,
                "Angle" => style.angle = value.parse().ok()?,
                "BorderStyle" => style.border_style = value.parse().ok()?,
                "Outline" => style.outline = value.parse().ok()?,
                "Shadow" => style.shadow = value.parse().ok()?,
                "Alignment" => style.alignment = value.parse().ok()?,
                "MarginL" => style.margin_l = value.parse().ok()?,
                "MarginR" => style.margin_r = value.parse().ok()?,
                "MarginV" => style.margin_v = value.parse().ok()?,
                "Encoding" => style.encoding = value.parse().ok()?,
                _ => {}
            }
        }
        Some(style)
    }
}

impl SectionParse for StylesSection {
    fn process_line(&mut self, line: Line) -> Result<(), Error> {
        if line.is_empty() {
            return Ok(());
        }

        let (key, value) = line.item().ok_or(ErrorKind::Invalid)?;
        match key {
            "Format" => {
                self.format = value.split(", ").map(String::from).collect();
                Ok(())
            }
            "Style" => {
                if self.format.is_empty() {
                    Err(ErrorKind::MissingStyleFormat.into())
                } else {
                    self.styles.push(
                        self.style_from_format(value)
                            .ok_or(ErrorKind::InvalidStyle)?,
                    );
                    Ok(())
                }
            }
            _ => Err(ErrorKind::Invalid.into()),
        }
    }
}

impl ToAss for StylesSection {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "[V4+ Styles]")?;
        // Yes, the order is hardcoded.
        writeln!(writer, "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding")?;
        for style in &self.styles {
            style.to_ass(writer)?;
        }
        Ok(())
    }
}

fn ass_timestamp_to_duration(s: &str) -> Option<Duration> {
    let (ts, subsec) = s.split_once('.')?;
    let mut units = ts.splitn(3, ':');
    let hours = units.next()?.parse::<u64>().ok()?;
    let minutes = units.next()?.parse::<u64>().ok()?;
    let seconds = units.next()?.parse::<u64>().ok()?;
    let cs = subsec.parse::<u32>().ok()?;
    Some(Duration::new(
        hours * 3600 + minutes * 60 + seconds,
        cs * 10_000_000,
    ))
}

struct AssDuration<'a>(&'a Duration);

impl<'a> Display for AssDuration<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // .ass files work with centiseconds instead of ms for some reason
        let centi = self.0.as_millis() / 10;
        write!(
            f,
            "{}:{:02}:{:02}.{:02}",
            centi / 360000,
            (centi / 6000) % 60,
            (centi / 100) % 60,
            centi % 100
        )
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum EventKind {
    #[default]
    Dialogue,
    Comment,
    Movie,
    Sound,
    Picture,
}

impl FromStr for EventKind {
    type Err = ErrorKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dialogue" => Ok(Self::Dialogue),
            "Comment" => Ok(Self::Comment),
            "Movie" => Ok(Self::Movie),
            "Sound" => Ok(Self::Sound),
            "Picture" => Ok(Self::Picture),
            _ => Err(ErrorKind::InvalidEventType),
        }
    }
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Dialogue => "Dialogue",
            EventKind::Comment => "Comment",
            EventKind::Movie => "Movie",
            EventKind::Sound => "Sound",
            EventKind::Picture => "Picture",
        }
    }

    /// Returns `true` if the event kind is [`Dialogue`].
    ///
    /// [`Dialogue`]: EventKind::Dialogue
    #[must_use]
    pub fn is_dialogue(&self) -> bool {
        matches!(self, Self::Dialogue)
    }

    /// Returns `true` if the event kind is [`Comment`].
    ///
    /// [`Comment`]: EventKind::Comment
    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self, Self::Comment)
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub layer: u8,
    pub start: Duration,
    pub end: Duration,
    pub style: String,
    pub name: String,
    pub margin_l: u16,
    pub margin_r: u16,
    pub margin_v: u16,
    pub effect: String,
    pub text: String,
}

impl Default for Event {
    fn default() -> Self {
        Self {
            kind: EventKind::Dialogue,
            layer: 0,
            start: Duration::ZERO,
            end: Duration::ZERO,
            style: String::from("Default"),
            name: String::new(),
            margin_l: 0,
            margin_r: 0,
            margin_v: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

impl ToAss for Event {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(
            writer,
            "{}: {},{},{},{},{},{},{},{},{},{}",
            self.kind.as_str(),
            self.layer,
            AssDuration(&self.start),
            AssDuration(&self.end),
            self.style,
            self.name,
            self.margin_l,
            self.margin_r,
            self.margin_v,
            self.effect,
            self.text,
        )
    }
}

impl Event {
    pub fn shift_by(&mut self, seconds: f32) {
        let duration = Duration::from_secs_f32(seconds.abs());
        if seconds < 0.0 {
            self.start = self.start.saturating_sub(duration);
            self.end = self.end.saturating_sub(duration);
        } else {
            self.start = self.start.saturating_add(duration);
            self.end = self.end.saturating_add(duration);
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventsSection {
    format: Vec<String>,
    pub events: Vec<Event>,
}

impl Default for EventsSection {
    fn default() -> Self {
        Self {
            format: "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
                .split(", ")
                .map(String::from)
                .collect(),
            events: Vec::new(),
        }
    }
}

impl EventsSection {
    fn new() -> Self {
        Self {
            format: Vec::new(),
            events: Vec::new(),
        }
    }

    fn event_from_format(&self, kind: EventKind, data: &str) -> Option<Event> {
        let mut event = Event {
            kind,
            ..Default::default()
        };

        // See comment above about future proofing
        for (name, value) in self
            .format
            .iter()
            .map(String::as_str)
            .zip(data.splitn(self.format.len(), ','))
        {
            match name {
                "Layer" => event.layer = value.parse().ok()?,
                "Start" => event.start = ass_timestamp_to_duration(value)?,
                "End" => event.end = ass_timestamp_to_duration(value)?,
                "Style" => event.style = value.to_owned(),
                "Name" => event.name = value.to_owned(),
                "MarginL" => event.margin_l = value.parse().ok()?,
                "MarginR" => event.margin_r = value.parse().ok()?,
                "MarginV" => event.margin_v = value.parse().ok()?,
                "Effect" => event.effect = value.to_owned(),
                "Text" => event.text = value.to_owned(),
                _ => {}
            }
        }
        Some(event)
    }

    pub fn remove_comments(&mut self) {
        self.events.retain(|e| !e.kind.is_comment());
    }
}

impl SectionParse for EventsSection {
    fn process_line(&mut self, line: Line) -> Result<(), Error> {
        if line.is_empty() {
            return Ok(());
        }

        let (key, value) = line.item().ok_or(ErrorKind::Invalid)?;
        if key == "Format" {
            self.format = value.split(", ").map(String::from).collect();
            return Ok(());
        }

        let event_kind = key.parse::<EventKind>()?;
        if self.format.is_empty() {
            Err(ErrorKind::MissingStyleFormat.into())
        } else {
            self.events.push(
                self.event_from_format(event_kind, value)
                    .ok_or(ErrorKind::InvalidStyle)?,
            );
            Ok(())
        }
    }
}

impl ToAss for EventsSection {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "[Events]")?;
        writeln!(
            writer,
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
        )?;
        for event in &self.events {
            event.to_ass(writer)?;
        }
        Ok(())
    }
}

/// A section in the .ass file.
///
/// This is denoted by a section key.
#[derive(Debug, Clone)]
pub enum Section {
    /// The script info of the file, denoted by `[Script Info]`.
    ScriptInfo(ScriptInfo),
    /// The styles section of the file, denoted by `[V4+ Styles]`.
    Styles(StylesSection),
    /// The event section of the file, denoted by `[Events]`.
    Events(EventsSection),
    /// A generic section that doesn't have any special meaning.
    Generic(GenericSection),
}

impl Section {
    pub fn as_events(&self) -> Option<&EventsSection> {
        if let Self::Events(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_events_mut(&mut self) -> Option<&mut EventsSection> {
        if let Self::Events(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn try_into_events(self) -> Result<EventsSection, Self> {
        if let Self::Events(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    pub fn as_styles(&self) -> Option<&StylesSection> {
        if let Self::Styles(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_styles_mut(&mut self) -> Option<&mut StylesSection> {
        if let Self::Styles(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn try_into_styles(self) -> Result<StylesSection, Self> {
        if let Self::Styles(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    pub fn as_script_info(&self) -> Option<&ScriptInfo> {
        if let Self::ScriptInfo(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_script_info_mut(&mut self) -> Option<&mut ScriptInfo> {
        if let Self::ScriptInfo(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn try_into_script_info(self) -> Result<ScriptInfo, Self> {
        if let Self::ScriptInfo(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    pub fn as_generic(&self) -> Option<&GenericSection> {
        if let Self::Generic(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_generic_mut(&mut self) -> Option<&mut GenericSection> {
        if let Self::Generic(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn try_into_generic(self) -> Result<GenericSection, Self> {
        if let Self::Generic(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }

    pub fn remove_comments(&mut self) {
        match self {
            Section::ScriptInfo(script_info) => script_info.remove_comments(),
            Section::Events(events_section) => events_section.remove_comments(),
            Section::Generic(generic_section) => generic_section.remove_comments(),
            _ => {}
        }
    }
}

impl SectionParse for Section {
    fn process_line(&mut self, line: Line) -> Result<(), Error> {
        match self {
            Section::ScriptInfo(script_info) => script_info.process_line(line),
            Section::Styles(styles_section) => styles_section.process_line(line),
            Section::Generic(generic_section) => generic_section.process_line(line),
            Section::Events(events_section) => events_section.process_line(line),
        }
    }
}

impl ToAss for Section {
    fn to_ass<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Section::ScriptInfo(script_info) => script_info.to_ass(writer),
            Section::Styles(styles_section) => {
                styles_section.to_ass(writer)?;
                writeln!(writer)
            }
            Section::Events(events_section) => {
                events_section.to_ass(writer)?;
                writeln!(writer)
            }
            Section::Generic(generic_section) => generic_section.to_ass(writer),
        }
    }
}

fn srt_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"<(i|b|u|s)>(.+)</(?:i|b|u|s)>"#).unwrap())
}

fn srt_to_ass(line: &str) -> String {
    // Replace HTML tags with proper ASS tags
    struct ReplaceTags;

    impl regex::Replacer for ReplaceTags {
        fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
            // {\b1}text{\b0}
            let tag = &caps[1];
            dst.push_str("{\\");
            dst.push_str(tag);
            dst.push_str("1}");
            dst.push_str(&caps[2]);
            dst.push_str("{\\");
            dst.push_str(tag);
            dst.push_str("0}");
        }
    }

    srt_tag_regex()
        .replace_all(line, ReplaceTags)
        .replace("\n", r#"\N"#)
}

/// A parsed .ass subtitle file.
///
/// Only .ass v4+ is supported
#[derive(Debug, Clone)]
pub struct Ass {
    pub(crate) sections: Vec<Section>,
}

/// Returns the string inside a `[Title]` block (e.g. "Title").
fn get_generic_section_title(s: &str) -> Option<&str> {
    s.strip_prefix('[')?.strip_suffix(']')
}

impl Ass {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = std::fs::File::open(path)?;
        Self::from_reader(std::io::BufReader::new(file))
    }

    pub fn from_reader<R: BufRead>(mut reader: R) -> Result<Self, Error> {
        let mut sections = Vec::<Section>::new();

        // First line has to be [Script Info]
        // It also optionally has a UTF-8 BOM
        let mut buf = String::new();
        reader.read_line(&mut buf)?;

        let cleaned = buf
            .strip_prefix('\u{feff}')
            .unwrap_or(buf.as_str())
            .trim_end();
        if cleaned == "[Script Info]" {
            sections.push(Section::ScriptInfo(ScriptInfo::new()));
        } else {
            return Err(Error {
                kind: ErrorKind::MissingScriptInfo,
                line: 1,
            });
        }

        for (index, line) in reader.lines().enumerate() {
            let line = line?;
            let line_number = index + 2;

            if line == "[V4+ Styles]" {
                sections.push(Section::Styles(StylesSection::new()));
            } else if line == "[Events]" {
                sections.push(Section::Events(EventsSection::new()));
            } else if let Some(title) = get_generic_section_title(&line) {
                sections.push(Section::Generic(GenericSection::new(title)));
            } else if let Some(section) = sections.last_mut() {
                let Some(parsed) = Line::parse(&line) else {
                    continue;
                };
                section
                    .process_line(parsed)
                    .map_err(|e| e.with_line(line_number))?;
            } else {
                return Err(Error {
                    kind: ErrorKind::Invalid,
                    line: line_number,
                });
            }
        }

        Ok(Self { sections })
    }

    pub fn from_srt(dialogue: Vec<crate::srt::Dialogue>) -> Self {
        let mut sections = Vec::with_capacity(3);
        sections.push(Section::ScriptInfo(ScriptInfo::default()));
        let mut styles = StylesSection::default();
        styles.styles.clear();
        let mut style = Style::program_default();
        if dialogue
            .iter()
            .any(|s| crate::japanese::contains_japanese(&s.text))
        {
            style.bold = true;
            style.name = String::from("Yu Gothic UI");
        }
        styles.styles.push(style);
        sections.push(Section::Styles(styles));
        sections.push(Section::Events(EventsSection {
            events: dialogue
                .into_iter()
                .map(|d| Event {
                    text: srt_to_ass(&d.text),
                    start: d.start,
                    end: d.end,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
        Self { sections }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut fp = std::fs::File::create(path)?;
        for section in &self.sections {
            section.to_ass(&mut fp)?;
        }
        Ok(())
    }

    pub fn save_to_writer<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()> {
        for section in &self.sections {
            section.to_ass(&mut writer)?;
        }
        Ok(())
    }

    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.sections
            .iter()
            .filter_map(|s| s.as_events())
            .flat_map(|e| e.events.iter())
    }

    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.sections
            .iter_mut()
            .filter_map(|s| s.as_events_mut())
            .flat_map(|e| e.events.iter_mut())
    }
}

impl FromStr for Ass {
    type Err = Error;

    fn from_str(buf: &str) -> Result<Self, Self::Err> {
        if !buf.starts_with("[Script Info]") {
            return Err(Error {
                kind: ErrorKind::MissingScriptInfo,
                line: 1,
            });
        }

        let mut sections = Vec::new();
        for (index, line) in buf.lines().enumerate() {
            let line_number = index + 1;
            if line == "[Script Info]" {
                sections.push(Section::ScriptInfo(ScriptInfo::new()));
            } else if line == "[V4+ Styles]" {
                sections.push(Section::Styles(StylesSection::new()));
            } else if line == "[Events]" {
                sections.push(Section::Events(EventsSection::new()));
            } else if let Some(title) = get_generic_section_title(line) {
                sections.push(Section::Generic(GenericSection::new(title)));
            } else if let Some(section) = sections.last_mut() {
                let Some(parsed) = Line::parse(line) else {
                    continue;
                };
                section
                    .process_line(parsed)
                    .map_err(|e| e.with_line(line_number))?;
            } else {
                return Err(Error {
                    kind: ErrorKind::Invalid,
                    line: line_number,
                });
            }
        }

        Ok(Self { sections })
    }
}
