//! The preset table: what a set is when nobody has changed anything.
//!
//! Six rows in `config/export_presets.toml`, PM-owned and versioned, and the loader is what holds
//! the file to the contract rather than a digest. A studio may **tighten** a bound and may never
//! widen one - the shape phases 21 to 29 established, applied to the one table in this phase that
//! decides what a delivered file looks like.
//!
//! ## The whole file is refused, never one row
//!
//! A preset table that silently dropped its album row would hand a studio the gallery preset for
//! their album: eight-bit sRGB to a print lab, which is where the banding in a gradient sky comes
//! from. Phases 24 to 28 made the same call about their own policy tables and for the same reason -
//! a partially-loaded policy is a policy nobody chose.

use std::collections::BTreeMap;

use aura_core::contract::delivery::{
    DeliveryColour, ExportSet, FileFormat, ImageId, NamingTemplate, OutputSharpen, Resize,
    MAX_JPEG_QUALITY, MIN_JPEG_QUALITY,
};
use aura_core::AuraResult;
use serde::Deserialize;

use crate::errors::job_refused;

/// The presets this build ships, as a file.
const BUILT_IN: &str = include_str!("../config/export_presets.toml");

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    /// The set's name.
    pub name: String,
    /// What kind of file.
    pub format: FileFormat,
    /// JPEG quality.
    pub quality: u8,
    /// Output space.
    pub colour: DeliveryColour,
    /// Bits per sample.
    pub bit_depth: u8,
    /// How large.
    pub resize: Resize,
    /// Output sharpening.
    pub sharpen: OutputSharpen,
    /// The naming template.
    pub naming: NamingTemplate,
    /// Whether an XMP sidecar goes beside each file.
    pub sidecar: bool,
    /// The argued-over half: why this row is what it is. Rendered in the export dialog.
    pub reason: String,
}

impl Preset {
    /// Turn a preset into a set over a list of photographs.
    #[must_use]
    pub fn with(&self, images: Vec<ImageId>) -> ExportSet {
        ExportSet {
            name: self.name.clone(),
            images,
            format: self.format,
            quality: self.quality,
            resize: self.resize,
            sharpen: self.sharpen,
            naming: self.naming.clone(),
            colour: self.colour,
            bit_depth: self.bit_depth,
            sidecar: self.sidecar,
        }
    }
}

/// The loaded table.
#[derive(Debug, Clone)]
pub struct Presets {
    version: u16,
    rows: BTreeMap<String, Preset>,
}

impl Presets {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the built-in table does not parse, which is a build defect rather
    /// than a studio's mistake and is why this is fallible at all.
    pub fn built_in() -> AuraResult<Self> {
        Self::parse(BUILT_IN)
    }

    /// Parse and check a table.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the file does not parse, has no rows, has two rows with one name,
    /// or has a row that asks for something the contract does not permit.
    pub fn parse(text: &str) -> AuraResult<Self> {
        #[derive(Deserialize)]
        struct File {
            version: u16,
            preset: Vec<Row>,
        }
        #[derive(Deserialize)]
        struct Row {
            name: String,
            format: String,
            quality: u8,
            colour: String,
            bit_depth: u8,
            resize: String,
            sharpen: String,
            naming: String,
            #[serde(default)]
            sidecar: bool,
            reason: String,
        }

        let file: File = toml::from_str(text)
            .map_err(|e| job_refused(format!("export presets did not parse: {e}")))?;
        if file.preset.is_empty() {
            return Err(job_refused("export presets table has no rows"));
        }

        let mut rows = BTreeMap::new();
        for row in file.preset {
            let format = FileFormat::parse(&row.format)?;
            let colour = DeliveryColour::parse(&row.colour)?;
            let sharpen = OutputSharpen::parse(&row.sharpen)?;
            let resize = parse_resize(&row.resize)?;
            let naming = NamingTemplate::parse(&row.naming)?;

            // The contract owns every bound; this file may only choose inside them.
            if format.is_lossy() && !(MIN_JPEG_QUALITY..=MAX_JPEG_QUALITY).contains(&row.quality) {
                return Err(job_refused(format!(
                    "preset `{}` asks for quality {} outside {MIN_JPEG_QUALITY}..={MAX_JPEG_QUALITY}",
                    row.name, row.quality
                )));
            }
            if row.bit_depth != 8 && !(row.bit_depth == 16 && format.supports_sixteen_bit()) {
                return Err(job_refused(format!(
                    "preset `{}` asks for {} bits on a {}",
                    row.name, row.bit_depth, format
                )));
            }
            resize.validate()?;
            if row.reason.trim().is_empty() {
                // Phase 10's rule: a weight table is a product decision and needs a written reason
                // per row. A preset nobody can explain is a preset nobody can argue with.
                return Err(job_refused(format!(
                    "preset `{}` has no written reason",
                    row.name
                )));
            }

            let preset = Preset {
                name: row.name.clone(),
                format,
                quality: row.quality,
                colour,
                bit_depth: row.bit_depth,
                resize,
                sharpen,
                naming,
                sidecar: row.sidecar,
                reason: row.reason,
            };
            if rows.insert(row.name.clone(), preset).is_some() {
                return Err(job_refused(format!("two presets named `{}`", row.name)));
            }
        }

        Ok(Self {
            version: file.version,
            rows,
        })
    }

    /// Which version of the table this is. Bumped when a row changes, so an export summary can say
    /// which presets a delivery was made under.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// One preset by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Preset> {
        self.rows.get(name)
    }

    /// Every preset, in name order.
    #[must_use]
    pub fn all(&self) -> Vec<&Preset> {
        self.rows.values().collect()
    }
}

fn parse_resize(text: &str) -> AuraResult<Resize> {
    if text == "full" {
        return Ok(Resize::Full);
    }
    if let Some(rest) = text.strip_prefix("long_edge:") {
        let pixels: u32 = rest
            .parse()
            .map_err(|_| job_refused(format!("`{text}` is not a long edge")))?;
        return Ok(Resize::LongEdge { pixels });
    }
    if let Some(rest) = text.strip_prefix("fit:") {
        let mut parts = rest.split('x');
        let (Some(w), Some(h), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(job_refused(format!("`{text}` is not a fit box")));
        };
        let width: u32 = w
            .parse()
            .map_err(|_| job_refused(format!("`{text}` has a bad width")))?;
        let height: u32 = h
            .parse()
            .map_err(|_| job_refused(format!("`{text}` has a bad height")))?;
        return Ok(Resize::Fit { width, height });
    }
    Err(job_refused(format!("`{text}` is not a resize")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_loads_and_has_the_five_sets_section_two_names() {
        let p = Presets::built_in().unwrap();
        for name in ["gallery", "album", "social", "teaser", "bw"] {
            assert!(p.get(name).is_some(), "no `{name}` preset");
        }
        assert_eq!(p.version(), 1);
    }

    #[test]
    fn the_album_goes_to_a_lab_at_sixteen_bits_and_the_gallery_does_not() {
        let p = Presets::built_in().unwrap();
        let album = p.get("album").unwrap();
        assert_eq!(album.format, FileFormat::Tiff);
        assert_eq!(album.bit_depth, 16);
        assert_eq!(album.colour, DeliveryColour::AdobeRgb);
        assert_eq!(album.sharpen, OutputSharpen::Print);

        let gallery = p.get("gallery").unwrap();
        assert_eq!(gallery.format, FileFormat::Jpeg);
        assert_eq!(gallery.bit_depth, 8);
        assert_eq!(gallery.colour, DeliveryColour::Srgb);
        assert!(
            gallery.quality >= 90,
            "skin artefacts at 100 % are the complaint"
        );
    }

    #[test]
    fn the_handoff_preset_writes_a_sidecar_and_no_output_sharpening() {
        // What leaves for another editor is going to be edited again, and both of those are
        // decisions the next editor should make.
        let p = Presets::built_in().unwrap();
        let h = p.get("handoff").unwrap();
        assert!(h.sidecar);
        assert_eq!(h.sharpen, OutputSharpen::None);
        assert_eq!(h.naming.as_str(), NamingTemplate::HANDOFF_DEFAULT);
    }

    #[test]
    fn every_row_carries_a_written_reason() {
        // Phase 10's rule. A preset nobody can explain is a preset nobody can argue with.
        for row in Presets::built_in().unwrap().all() {
            assert!(row.reason.len() > 20, "`{}` has a thin reason", row.name);
        }
    }

    #[test]
    fn a_row_that_widens_a_bound_refuses_the_whole_file() {
        // Not just the row. A table that silently dropped its album row would hand a studio the
        // gallery preset for their album.
        let bad = r#"
version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 40
colour = "srgb"
bit_depth = 8
resize = "full"
sharpen = "screen"
naming = "{seq}"
reason = "quality below the contract's floor"
"#;
        assert!(Presets::parse(bad).is_err());

        let bad_depth = r#"
version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 92
colour = "srgb"
bit_depth = 16
resize = "full"
sharpen = "screen"
naming = "{seq}"
reason = "sixteen bits in a jpeg"
"#;
        assert!(Presets::parse(bad_depth).is_err());

        let bad_name = r#"
version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 92
colour = "srgb"
bit_depth = 8
resize = "full"
sharpen = "screen"
naming = "{date}/{seq}"
reason = "a template that names a folder"
"#;
        assert!(Presets::parse(bad_name).is_err());
    }

    #[test]
    fn a_row_with_no_reason_is_refused() {
        let bad = r#"
version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 92
colour = "srgb"
bit_depth = 8
resize = "full"
sharpen = "screen"
naming = "{seq}"
reason = "   "
"#;
        assert!(Presets::parse(bad).is_err());
    }

    #[test]
    fn every_resize_form_parses_and_a_nonsense_one_does_not() {
        assert_eq!(parse_resize("full").unwrap(), Resize::Full);
        assert_eq!(
            parse_resize("long_edge:2048").unwrap(),
            Resize::LongEdge { pixels: 2048 }
        );
        assert_eq!(
            parse_resize("fit:1200x800").unwrap(),
            Resize::Fit {
                width: 1200,
                height: 800
            }
        );
        assert!(parse_resize("2048").is_err());
        assert!(parse_resize("fit:1200").is_err());
        assert!(parse_resize("fit:1200x800x600").is_err());
    }
}
