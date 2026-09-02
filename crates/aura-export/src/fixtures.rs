//! Authored plates and a scripted source, so the export loop can be driven without a renderer.
//!
//! Every gate in this phase is measured against frames whose content was **chosen**: a chequer at
//! the Nyquist frequency to catch a resampler that aliases, a smooth gradient to catch one that
//! bands, a flat field to catch one that shifts, and a hard edge to catch a sharpener that rings.
//!
//! ## What a fixture here proves and what it does not
//!
//! It proves the writers, the resampler, the naming, the verification, the manifest and the store.
//! It says nothing about a wedding photograph, because there are no camera files in this repository
//! and no rendered gallery to compare against - which is phase 02's condition and phase 14's, still
//! open, reaching this phase.
//!
//! The one thing it does prove about pixels is the property that matters most here: **what comes
//! out of the file is what went into it**. A JPEG that decodes back to the plate within the DCT's
//! own rounding is a JPEG that will decode back to a photograph.

use std::collections::BTreeMap;
use std::path::PathBuf;

use aura_core::contract::delivery::{
    DeliveryColour, Destination, ExportJob, ExportSet, FileFormat, ImageId, NamingTemplate,
    OutputSharpen, Resize,
};
use aura_core::{AuraResult, ProjectId};

use crate::read::{Field, Frame, Rendered, Samples, Source};

/// What a plate is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plate {
    /// A flat mid grey. Catches a resampler or a transfer curve that shifts.
    Flat,
    /// A one-pixel chequer. Catches a resampler that darkens, and one that aliases.
    Chequer,
    /// A left-to-right ramp. Catches banding and a curve that quantises badly.
    Gradient,
    /// A hard vertical edge. Catches a sharpener that rings and one that does nothing.
    Edge,
    /// Saturated primaries in quadrants. Catches a colour-space mix-up.
    Primaries,
}

impl Plate {
    /// Every plate.
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::Chequer,
        Self::Gradient,
        Self::Edge,
        Self::Primaries,
    ];

    /// The sample at one position.
    #[must_use]
    pub fn sample(self, x: u32, y: u32, w: u32, h: u32) -> [u8; 3] {
        match self {
            Self::Flat => [128, 128, 128],
            Self::Chequer => {
                if (x + y).is_multiple_of(2) {
                    [255, 255, 255]
                } else {
                    [0, 0, 0]
                }
            }
            Self::Gradient => {
                let v = if w <= 1 {
                    0
                } else {
                    ((x * 255) / (w - 1)) as u8
                };
                [v, v, v]
            }
            Self::Edge => {
                if x * 2 < w {
                    [32, 32, 32]
                } else {
                    [224, 224, 224]
                }
            }
            Self::Primaries => {
                let left = x * 2 < w;
                let top = y * 2 < h;
                match (left, top) {
                    (true, true) => [220, 30, 40],
                    (false, true) => [30, 190, 60],
                    (true, false) => [40, 60, 210],
                    (false, false) => [235, 225, 200],
                }
            }
        }
    }
}

/// Build one plate.
#[must_use]
pub fn plate(kind: Plate, w: u32, h: u32, colour: DeliveryColour, bit_depth: u8) -> Rendered {
    let n = (w as usize) * (h as usize) * 3;
    let mut eight = Vec::with_capacity(n);
    for y in 0..h {
        for x in 0..w {
            eight.extend_from_slice(&kind.sample(x, y, w, h));
        }
    }
    let samples = if bit_depth == 16 {
        Samples::Sixteen(eight.iter().map(|&v| u16::from(v) * 257).collect())
    } else {
        Samples::Eight(eight)
    };
    Rendered {
        width: w,
        height: h,
        data: samples,
        colour,
        render_hash: format!("{:0>64}", format!("{kind:?}").to_ascii_lowercase()),
    }
}

/// A field over a scripted wedding.
#[derive(Debug, Clone)]
pub struct ScriptedField {
    frames: BTreeMap<String, Frame>,
    couple: Option<String>,
    photos: u32,
    selected: u32,
    qc_report: Option<PathBuf>,
}

impl ScriptedField {
    /// A field with a couple and a set of frames.
    #[must_use]
    pub fn new(couple: Option<&str>, photos: u32, selected: u32) -> Self {
        Self {
            frames: BTreeMap::new(),
            couple: couple.map(str::to_owned),
            photos,
            selected,
            qc_report: None,
        }
    }

    /// Add one frame.
    #[must_use]
    pub fn with_frame(mut self, image: ImageId, frame: Frame) -> Self {
        self.frames.insert(image.to_db(), frame);
        self
    }

    /// Point the field at an archived QC report.
    #[must_use]
    pub fn with_qc_report(mut self, path: PathBuf) -> Self {
        self.qc_report = Some(path);
        self
    }
}

impl Field for ScriptedField {
    fn couple(&self) -> Option<String> {
        self.couple.clone()
    }
    fn photos(&self) -> u32 {
        self.photos
    }
    fn selected(&self) -> u32 {
        self.selected
    }
    fn frame(&self, image: ImageId) -> Frame {
        self.frames
            .get(&image.to_db())
            .cloned()
            .unwrap_or_else(|| Frame::bare(image))
    }
    fn qc_report_path(&self) -> Option<PathBuf> {
        self.qc_report.clone()
    }
    fn engine_versions(&self) -> Vec<(String, String)> {
        vec![
            ("app".to_owned(), "0.1.0".to_owned()),
            ("export".to_owned(), crate::ENGINE.to_owned()),
        ]
    }
}

/// A source that answers with authored plates, and can be made to fail on one frame.
#[derive(Debug, Clone)]
pub struct ScriptedSource {
    plates: BTreeMap<String, (Plate, u32, u32)>,
    default: (Plate, u32, u32),
    fail: Vec<String>,
}

impl ScriptedSource {
    /// A source whose default answer is one plate at one size.
    #[must_use]
    pub fn new(default: Plate, w: u32, h: u32) -> Self {
        Self {
            plates: BTreeMap::new(),
            default: (default, w, h),
            fail: Vec::new(),
        }
    }

    /// Give one frame its own plate.
    #[must_use]
    pub fn with(mut self, image: ImageId, kind: Plate, w: u32, h: u32) -> Self {
        self.plates.insert(image.to_db(), (kind, w, h));
        self
    }

    /// Make one frame refuse to render, which is what an unreadable original looks like.
    #[must_use]
    pub fn failing(mut self, image: ImageId) -> Self {
        self.fail.push(image.to_db());
        self
    }
}

impl Source for ScriptedSource {
    fn render(
        &self,
        _project: ProjectId,
        image: ImageId,
        colour: DeliveryColour,
        bit_depth: u8,
    ) -> AuraResult<Rendered> {
        if self.fail.contains(&image.to_db()) {
            return Err(crate::errors::render_failed(format!(
                "scripted failure for {}",
                image.to_db()
            )));
        }
        let (kind, w, h) = self
            .plates
            .get(&image.to_db())
            .copied()
            .unwrap_or(self.default);
        Ok(plate(kind, w, h, colour, bit_depth))
    }
}

/// A one-set job over a folder, for a test that does not care about the shape of the job.
#[must_use]
pub fn simple_job(images: Vec<ImageId>, root: PathBuf, format: FileFormat) -> ExportJob {
    ExportJob::new(
        vec![ExportSet {
            name: "gallery".to_owned(),
            images,
            format,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::None,
            naming: NamingTemplate::parse("{seq}").unwrap_or_default(),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        }],
        Destination::Folder { path: root },
    )
}

/// A wedding of `n` photographs with the ids a test needs to refer to.
#[must_use]
pub fn wedding(n: usize) -> Vec<ImageId> {
    (0..n).map(|_| ImageId::new()).collect()
}
