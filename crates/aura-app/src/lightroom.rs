//! Reading a Lightroom Classic catalogue: which photographs it holds and the develop settings a
//! photographer gave each one. ADR-0104.
//!
//! A `.lrcat` is a `SQLite` database. The settings live in `Adobe_imageDevelopSettings.text` as a
//! Lua table (`s = { Exposure2012 = 0.35, ToneCurvePV2012 = { 0, 0, 255, 255 }, ... }`), which
//! [`parse_settings`] reads into JSON. Only the top level of that table is a photograph's
//! develop settings: nested tables hold the camera profile's own look, masks and spot removal,
//! and a number found inside one of those is not a slider the photographer moved.
//!
//! The catalogue is copied to a temporary file and opened read-only, so a Lightroom that has it
//! open is never blocked and the original is never written.
use std::path::{Path, PathBuf};

use aura_core::AuraError;
use rusqlite::OpenFlags;
use serde_json::{Map, Value};

fn invalid(message: impl Into<String>) -> AuraError {
    let message = message.into();
    let mut error = aura_core::errors::render::recipe_invalid("lightroom catalogue", &message);
    error.user_message = message;
    error
}

/// One photograph in a catalogue.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogPhoto {
    /// Where Lightroom last saw the original.
    pub path: PathBuf,
    /// `RAW`, `JPG`, `HEIC`, `TIFF`, `DNG`...
    pub format: String,
    /// Its develop settings, top level only.
    pub settings: Map<String, Value>,
    /// ISO, when the catalogue recorded it.
    pub iso: Option<f32>,
}

/// Everything read from one catalogue.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    pub photos: Vec<CatalogPhoto>,
    /// Photographs whose settings could not be read.
    pub unreadable: usize,
}

/// A small recursive-descent reader for the Lua tables Lightroom writes.
struct Lua<'a> {
    text: &'a [u8],
    at: usize,
}

impl Lua<'_> {
    fn skip(&mut self) {
        while let Some(c) = self.text.get(self.at) {
            if c.is_ascii_whitespace() || *c == b',' || *c == b';' {
                self.at += 1;
            } else if self.text.get(self.at..self.at + 2) == Some(b"--") {
                while self.text.get(self.at).is_some_and(|c| *c != b'\n') {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            self.at += 1;
        }
        (self.at > start).then(|| {
            String::from_utf8_lossy(self.text.get(start..self.at).unwrap_or_default()).into_owned()
        })
    }

    fn string(&mut self) -> Result<String, String> {
        let quote = self.peek().ok_or("end of text")?;
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.at += 1;
            if c == quote {
                break;
            }
            if c == b'\\' {
                let next = self.peek().ok_or("unterminated escape")?;
                self.at += 1;
                out.push(match next {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    other => other,
                });
            } else {
                out.push(c);
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.at += 1;
        }
        let text =
            String::from_utf8_lossy(self.text.get(start..self.at).unwrap_or_default()).into_owned();
        let value: f64 = text.parse().map_err(|_| format!("bad number {text}"))?;
        Ok(serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > 64 {
            return Err("nested too deeply".into());
        }
        self.skip();
        match self.peek().ok_or("end of text")? {
            b'{' => self.table(depth + 1),
            b'"' | b'\'' => self.string().map(Value::String),
            c if c.is_ascii_digit() || c == b'-' || c == b'.' => self.number(),
            _ => match self.ident().as_deref() {
                Some("true") => Ok(Value::Bool(true)),
                Some("false") => Ok(Value::Bool(false)),
                Some("nil") => Ok(Value::Null),
                other => Err(format!("unexpected {other:?}")),
            },
        }
    }

    fn table(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1; // '{'
        let mut keyed = Map::new();
        let mut list = Vec::new();
        loop {
            self.skip();
            match self.peek().ok_or("unterminated table")? {
                b'}' => {
                    self.at += 1;
                    break;
                }
                b'[' => {
                    self.at += 1;
                    self.skip();
                    let key = match self.value(depth)? {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    self.skip();
                    if self.peek() == Some(b']') {
                        self.at += 1;
                    }
                    self.skip();
                    if self.peek() == Some(b'=') {
                        self.at += 1;
                    }
                    let value = self.value(depth)?;
                    keyed.insert(key, value);
                }
                c if c.is_ascii_alphabetic() || c == b'_' => {
                    let mark = self.at;
                    let name = self.ident().unwrap_or_default();
                    self.skip();
                    if self.peek() == Some(b'=') {
                        self.at += 1;
                        let value = self.value(depth)?;
                        keyed.insert(name, value);
                    } else {
                        self.at = mark;
                        list.push(self.value(depth)?);
                    }
                }
                _ => list.push(self.value(depth)?),
            }
        }
        Ok(if keyed.is_empty() {
            Value::Array(list)
        } else {
            Value::Object(keyed)
        })
    }
}

/// The develop settings in `text` (`s = { ... }`), as a JSON object.
///
/// # Errors
/// Text that is not a Lua table Lightroom would write.
pub fn parse_settings(text: &str) -> Result<Map<String, Value>, String> {
    let start = text.find('{').ok_or("no table")?;
    let mut lua = Lua {
        text: text.as_bytes(),
        at: start,
    };
    match lua.table(0)? {
        Value::Object(map) => Ok(map),
        Value::Array(list) if list.is_empty() => Ok(Map::new()),
        _ => Err("the settings are a list, not a table".into()),
    }
}

/// Read every photograph and its settings from the catalogue at `path`.
///
/// # Errors
/// The file cannot be copied or is not a Lightroom Classic catalogue.
pub fn read(path: &Path) -> Result<Catalog, AuraError> {
    if !path.is_file() {
        return Err(invalid(format!("{} is not a file", path.display())));
    }
    let copy = std::env::temp_dir().join(format!(
        "aura-lrcat-{}.lrcat",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::copy(path, &copy)
        .map_err(|e| invalid(format!("Could not read the catalogue: {e}")))?;
    let result = read_copy(&copy);
    let _ = std::fs::remove_file(&copy);
    result
}

fn read_copy(path: &Path) -> Result<Catalog, AuraError> {
    let conn = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| invalid(format!("Not a Lightroom catalogue: {e}")))?;
    let has_exif = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'AgHarvestedExifMetadata'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let sql = format!(
        "SELECT r.absolutePath || f.pathFromRoot || fi.baseName || '.' || fi.extension,
                i.fileFormat, d.text, {}
           FROM Adobe_images i
           JOIN AgLibraryFile fi ON fi.id_local = i.rootFile
           JOIN AgLibraryFolder f ON f.id_local = fi.folder
           JOIN AgLibraryRootFolder r ON r.id_local = f.rootFolder
           LEFT JOIN Adobe_imageDevelopSettings d ON d.image = i.id_local
           {}",
        if has_exif { "e.isoSpeedRating" } else { "NULL" },
        if has_exif {
            "LEFT JOIN AgHarvestedExifMetadata e ON e.image = i.id_local"
        } else {
            ""
        }
    );
    let mut statement = conn
        .prepare(&sql)
        .map_err(|e| invalid(format!("Not a Lightroom Classic catalogue: {e}")))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<f64>>(3)?,
            ))
        })
        .map_err(|e| invalid(format!("Could not read the catalogue: {e}")))?;
    let mut photos = Vec::new();
    let mut unreadable = 0;
    for row in rows {
        let Ok((Some(path), format, Some(text), iso)) = row else {
            continue;
        };
        match parse_settings(&text) {
            Ok(settings) => photos.push(CatalogPhoto {
                path: PathBuf::from(path),
                format: format.unwrap_or_default(),
                settings,
                #[allow(clippy::cast_possible_truncation)]
                iso: iso.map(|v| v as f32),
            }),
            Err(_) => unreadable += 1,
        }
    }
    Ok(Catalog { photos, unreadable })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_settings_lightroom_writes() {
        let text = r#"s = { AutoLateralCA = 1,
Blacks2012 = -25,
CameraProfile = "Adobe Standard",
ConvertToGrayscale = false,
Exposure2012 = 0.34,
Look = { Amount = 1,
Group = { ["x-default"] = "Profiles" },
Parameters = { Exposure2012 = 2, ToneCurvePV2012 = { 0, 0, 255, 255 } } },
ToneCurvePV2012 = { 0,
0,
64,
70,
255,
255 },
LuminanceSmoothing = 1e-05,
Name = "it's \"quoted\"",
}
"#;
        let s = parse_settings(text).unwrap();
        assert_eq!(s["Blacks2012"].as_f64(), Some(-25.0));
        assert_eq!(s["Exposure2012"].as_f64(), Some(0.34));
        assert_eq!(s["ConvertToGrayscale"], Value::Bool(false));
        assert_eq!(s["CameraProfile"], "Adobe Standard");
        assert_eq!(s["ToneCurvePV2012"].as_array().map(Vec::len), Some(6));
        // The profile's own look is nested and does not overwrite the photograph's exposure.
        assert_eq!(s["Look"]["Parameters"]["Exposure2012"].as_f64(), Some(2.0));
        assert_eq!(s["Look"]["Group"]["x-default"], "Profiles");
        assert_eq!(s["Name"], "it's \"quoted\"");
        assert!(s["LuminanceSmoothing"].as_f64().unwrap() < 1e-4);
    }

    #[test]
    fn a_missing_or_foreign_file_is_a_reason() {
        assert!(read(Path::new("no-such.lrcat")).is_err());
        assert!(parse_settings("nothing here").is_err());
    }
}
