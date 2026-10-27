//! The hundred ADR-0150 tools on real photographs, through the application's own commands.
//!
//! Ignored by default. Point it at a folder of photographs and an output folder:
//!
//! ```text
//! AURA_EVOTO_PHOTOS="H:/AURA EDIT" AURA_EVOTO_OUT=F:/aura-evoto100/report \
//!   cargo test --release -p aura-app --test evoto100_photos -- --ignored --nocapture
//! ```
//!
//! Every photograph is imported into an isolated catalogue (by reference: nothing is copied or
//! changed in the source folder) and parsed once to sort it: how many faces, how large the main
//! one is, whether there is a beard, open sky, bare arms. Each tool is then applied to the
//! photographs that suit it exactly as its panel would apply it - `save_studio_finish`, the native
//! retouch stack, or the workflow commands - rendered through `render_image`, and compared with
//! the same photograph untouched:
//!
//! * `changed` - the share of pixels that moved by three levels or more;
//! * `mean` - the mean change over the whole frame, in levels of 255;
//! * `inside` - the share of all the change that landed inside the region the tool is for;
//! * `ms` - the render's wall-clock time.
//!
//! A before-and-after crop of every tool is written to `crops/`, and `results.json` carries
//! every number. `AURA_EVOTO_ONLY=a,b` limits the run to some tools, `AURA_EVOTO_PER_TOOL` sets how
//! many photographs each tool is tried on (default 3).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput, RenderImageInput};
use aura_app::evoto_workflow::{self, BlinkInput, ExportInput, FramingInput};
use aura_app::finish_commands::{self, SaveStudioFinishInput, StudioFinishInput};
use aura_app::native_retouch::{self, RetouchInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_portrait::{PortraitMap, Region};
use aura_recipe::retouch_tools::{Edit, Tool};
use aura_recipe::studio_finish::StudioFinish;
use serde_json::{json, Value};

// ---- the catalogue of tools -------------------------------------------------------------

/// Which photographs a tool is tried on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suits {
    /// The largest single faces.
    Portrait,
    /// Three faces or more.
    Group,
    /// A beard or stubble.
    Beard,
    /// Bare arms, hands or shoulders.
    BodySkin,
    /// A whole person in the frame.
    Person,
    /// Open sky.
    Sky,
    /// Any photograph.
    Any,
}

#[derive(Debug, Clone)]
enum Apply {
    /// A patch merged into the studio finish's JSON.
    Finish(Value),
    /// A hand-placed tool over a region.
    Effect(Tool, Region, f32),
    /// Face picker: one tool over the second face only.
    FacePicker,
    Framing(&'static str),
    Export(&'static str, Option<u32>),
    Blink,
}

#[derive(Debug, Clone)]
struct Case {
    id: &'static str,
    group: &'static str,
    label: &'static str,
    apply: Apply,
    suits: Suits,
    /// The regions all the change should stay inside; empty is anywhere.
    regions: Vec<Region>,
}

fn finish(group: &str, key: &str, value: f32) -> Apply {
    Apply::Finish(json!({ group: { key: value } }))
}

fn case(
    id: &'static str,
    group: &'static str,
    label: &'static str,
    apply: Apply,
    suits: Suits,
    regions: &[Region],
) -> Case {
    Case {
        id,
        group,
        label,
        apply,
        suits,
        regions: regions.to_vec(),
    }
}

fn catalogue() -> Vec<Case> {
    use Region as R;
    use Suits as S;
    let head = [
        R::Face,
        R::Hair,
        R::Skin,
        R::Neck,
        R::Eyes,
        R::Eyebrows,
        R::Lips,
        R::Mouth,
        R::Background,
        R::Body,
    ];
    let face = [
        R::Face,
        R::Skin,
        R::Eyes,
        R::Iris,
        R::Sclera,
        R::Eyebrows,
        R::UnderEyes,
        R::Nose,
        R::Lips,
        R::Teeth,
        R::Mouth,
        R::FacialHair,
    ];
    let person = [
        R::Body,
        R::Hair,
        R::Face,
        R::Skin,
        R::BodySkin,
        R::Clothing,
        R::Neck,
    ];
    let mut v = Vec::new();
    // A and B: face shape.
    for (id, key, label) in [
        ("A1", "eyeHeight", "Eye height"),
        ("A2", "eyeTilt", "Eye tilt"),
        ("A3", "innerCorner", "Inner corners"),
        ("A4", "outerCorner", "Outer corners"),
        ("A5", "eyeSymmetry", "Eye symmetry"),
        ("A6", "noseBridge", "Nose bridge"),
        ("A7", "noseTip", "Nose tip"),
        ("A8", "noseLift", "Nose lift"),
        ("A9", "nostrils", "Nostrils"),
        ("A10", "cheekApples", "Cheek apples"),
        ("B1", "upperLip", "Upper lip"),
        ("B2", "lowerLip", "Lower lip"),
        ("B3", "mouthHeight", "Mouth position"),
        ("B4", "browHeight", "Brow height"),
        ("B5", "browTilt", "Brow arch"),
        ("B6", "browThickness", "Brow thickness"),
        ("B7", "temples", "Temples"),
        ("B8", "faceLength", "Lower face length"),
        ("B9", "chinWidth", "Chin width"),
        ("B10", "hairVolume", "Hair volume"),
    ] {
        let group = if id.starts_with('A') {
            "A face: eyes, nose, cheeks"
        } else {
            "B face: mouth, brows, shape"
        };
        v.push(case(
            id,
            group,
            label,
            finish("face", key, 80.0),
            S::Portrait,
            &head,
        ));
    }
    // C: body.
    for (id, key, label, value, suits) in [
        ("C1", "thighs", "Thighs", 80.0, S::Person),
        ("C2", "calves", "Calves", 80.0, S::Person),
        ("C3", "belly", "Flat stomach", 80.0, S::Person),
        ("C4", "hipLift", "Hip lift", 80.0, S::Person),
        ("C5", "shoulderSlope", "Shoulder line", 80.0, S::Person),
        ("C6", "torso", "Torso length", 80.0, S::Person),
        ("C7", "height", "Height", 80.0, S::Person),
        ("C8", "neckWidth", "Neck slim", 80.0, S::Portrait),
        ("C9", "doubleChin", "Double chin", 80.0, S::Portrait),
        ("C10", "headLevel", "Level head", 100.0, S::Portrait),
    ] {
        v.push(case(
            id,
            "C body",
            label,
            finish("body", key, value),
            suits,
            &[],
        ));
    }
    // D: skin.
    for (id, key, label) in [
        ("D1", "aegyoSal", "Aegyo-sal"),
        ("D2", "lipLines", "Lip lines"),
        ("D3", "glow", "Dewy glow"),
        ("D4", "stubble", "Stubble shadow"),
        ("D5", "rosy", "Rosy complexion"),
        ("D6", "teethEvening", "Even teeth"),
        ("D7", "featureSharpen", "Sharpen features"),
        ("D8", "beardDefine", "Beard definition"),
        ("D9", "colourSpill", "Clothing colour on skin"),
        ("D10", "pores", "Refine pores"),
    ] {
        let suits = if matches!(id, "D4" | "D8") {
            S::Beard
        } else {
            S::Portrait
        };
        v.push(case(
            id,
            "D skin",
            label,
            finish("skin", key, 100.0),
            suits,
            &face,
        ));
    }
    // E: makeup.
    for (id, key, label) in [
        ("E1", "eyeliner", "Eyeliner"),
        ("E2", "eyelinerWing", "Eyeliner wing"),
        ("E3", "lashes", "Fuller lashes"),
        ("E4", "lowerLashes", "Lower lashes"),
        ("E5", "lipLiner", "Lip liner"),
        ("E6", "lipOmbre", "Ombre lips"),
        ("E7", "highlighter", "Highlighter"),
        ("E8", "bronzer", "Bronzer"),
        ("E9", "enhance", "Enhance makeup"),
        ("E10", "browFill", "Fill brows"),
    ] {
        let apply = if id == "E2" {
            Apply::Finish(json!({ "makeup": { "eyeliner": 70.0, "eyelinerWing": 100.0 } }))
        } else {
            finish("makeup", key, 100.0)
        };
        v.push(case(id, "E makeup", label, apply, S::Portrait, &face));
    }
    // F: hair and body skin.
    for (id, key, label) in [
        ("F1", "gloss", "Hair shine"),
        ("F2", "parting", "Fill parting"),
        ("F3", "richness", "Hair colour depth"),
        ("F4", "edge", "Calm hair edge"),
    ] {
        v.push(case(
            id,
            "F hair and body",
            label,
            finish("hair", key, 100.0),
            S::Portrait,
            &[R::Hair, R::Background, R::Face],
        ));
    }
    v.push(case(
        "F5",
        "F hair and body",
        "Body definition",
        finish("hair", "definition", 100.0),
        S::BodySkin,
        &[R::BodySkin, R::Skin],
    ));
    for (id, tool, label) in [
        ("F6", Tool::BodyHairSoften, "Soften body hair"),
        ("F7", Tool::Cellulite, "Smooth cellulite"),
        ("F8", Tool::TanLine, "Even tan lines"),
        ("F9", Tool::HandRejuvenate, "Rejuvenate hands"),
        ("F10", Tool::KnuckleEven, "Even knuckles and elbows"),
    ] {
        v.push(case(
            id,
            "F hair and body",
            label,
            Apply::Effect(tool, R::BodySkin, 0.9),
            S::BodySkin,
            &[],
        ));
    }
    // G: attire.
    for (id, key, label, regions) in [
        (
            "G1",
            "gold",
            "Gold jewellery",
            &[R::Body, R::Clothing, R::Background][..],
        ),
        ("G2", "redRecovery", "Recover red fabric", &[][..]),
        (
            "G3",
            "veilClarity",
            "Veil clarity",
            &[
                R::Face,
                R::Skin,
                R::Eyes,
                R::Lips,
                R::Mouth,
                R::Eyebrows,
                R::Nose,
            ][..],
        ),
        ("G4", "embroidery", "Embroidery detail", &[R::Clothing][..]),
        (
            "G5",
            "garmentEdge",
            "Smooth garment edges",
            &[R::Clothing, R::Background][..],
        ),
        ("G6", "mehndi", "Deepen mehndi", &[R::Skin, R::BodySkin][..]),
        (
            "G7",
            "whiteGarment",
            "Clean white clothes",
            &[R::Clothing][..],
        ),
        ("G8", "darkGarment", "Dark suit detail", &[R::Clothing][..]),
        ("G9", "outfitColour", "Outfit colour", &[R::Clothing][..]),
    ] {
        let suits = if id == "G6" { S::BodySkin } else { S::Any };
        v.push(case(
            id,
            "G attire",
            label,
            finish("attire", key, 100.0),
            suits,
            regions,
        ));
    }
    v.push(case(
        "G10",
        "G attire",
        "Glasses reflection colour",
        Apply::Effect(Tool::LensReflection, R::Eyes, 0.9),
        S::Portrait,
        &[],
    ));
    // H: background.
    for (id, key, label, value) in [
        ("H1", "backdropGlow", "Backdrop light", 100.0),
        ("H3", "saturation", "Background saturation", -80.0),
        ("H4", "brightness", "Background brightness", -60.0),
        ("H5", "warmth", "Background warmth", 60.0),
        ("H6", "depthBlur", "Depth blur", 100.0),
        ("H8", "denoise", "Background noise", 100.0),
        ("H10", "lightWrap", "Light wrap", 100.0),
    ] {
        let regions: &[Region] = if id == "H10" {
            &person
        } else {
            &[R::Background, R::Sky]
        };
        v.push(case(
            id,
            "H background",
            label,
            finish("scene", key, value),
            S::Any,
            regions,
        ));
    }
    v.push(case("H2", "H background", "Colour, keep shadows", Apply::Finish(json!({ "background": { "mode": "keep_shadows", "colour": [0.96, 0.96, 0.96], "colour2": [1.0, 1.0, 1.0], "amount": 100.0, "feather": 30.0 } })), S::Person, &[R::Background, R::Sky]));
    v.push(case("H7", "H background", "Sky with clouds", Apply::Finish(json!({ "background": { "mode": "cloud_sky", "colour": [0.24, 0.48, 0.86], "colour2": [0.74, 0.86, 0.97], "amount": 100.0, "feather": 40.0 } })), S::Sky, &[R::Sky, R::Background]));
    v.push(case(
        "H9",
        "H background",
        "Remove a cast shadow",
        Apply::Effect(Tool::ShadowRemove, R::Background, 0.9),
        S::Any,
        &[],
    ));
    // I: people.
    for (id, key, label, value, suits) in [
        ("I1", "exposure", "People exposure", 60.0, S::Person),
        ("I2", "warmth", "People warmth", 60.0, S::Person),
        ("I3", "clarity", "People clarity", 80.0, S::Person),
        ("I4", "groupBalance", "Balance group faces", 100.0, S::Group),
        (
            "I5",
            "groupSkin",
            "Match group skin colour",
            100.0,
            S::Group,
        ),
        ("I6", "relight", "Relight", 80.0, S::Person),
        (
            "I7",
            "harshShadows",
            "Soften harsh shadows",
            100.0,
            S::Portrait,
        ),
        ("I8", "backlight", "Backlight rescue", 100.0, S::Person),
        ("I9", "faceVignette", "Face-aware vignette", 100.0, S::Any),
        (
            "I10",
            "faceBrightness",
            "Face brightness",
            60.0,
            S::Portrait,
        ),
    ] {
        let regions: &[Region] = match id {
            "I9" => &[],
            "I4" | "I5" | "I7" | "I10" => &[
                R::Face,
                R::Skin,
                R::Neck,
                R::Body,
                R::Hair,
                R::Eyes,
                R::Lips,
                R::Mouth,
                R::Eyebrows,
                R::Nose,
            ],
            _ => &person,
        };
        v.push(case(
            id,
            "I people",
            label,
            finish("subject", key, value),
            suits,
            regions,
        ));
    }
    // J: workflow.
    for (id, preset, label) in [
        ("J1", "id_35x45", "ID photo 35x45 on white"),
        ("J2", "headshot", "Auto crop: headshot 4:5"),
        ("J3", "level_eyes", "Level the eyes"),
    ] {
        v.push(case(
            id,
            "J workflow",
            label,
            Apply::Framing(preset),
            S::Portrait,
            &[],
        ));
    }
    v.push(case(
        "J4",
        "J workflow",
        "Transparent PNG cut-out",
        Apply::Export("png_cutout", None),
        S::Portrait,
        &[],
    ));
    v.push(case(
        "J5",
        "J workflow",
        "Instagram grid",
        Apply::Export("grid", Some(2)),
        S::Any,
        &[],
    ));
    v.push(case(
        "J6",
        "J workflow",
        "Carousel slides",
        Apply::Export("carousel", Some(3)),
        S::Group,
        &[],
    ));
    v.push(case(
        "J7",
        "J workflow",
        "Before and after",
        Apply::Export("before_after", None),
        S::Portrait,
        &[],
    ));
    v.push(case(
        "J8",
        "J workflow",
        "Enlarge 2x",
        Apply::Export("upscale_2x", None),
        S::Portrait,
        &[],
    ));
    v.push(case(
        "J9",
        "J workflow",
        "Face picker",
        Apply::FacePicker,
        S::Group,
        &[],
    ));
    v.push(case(
        "J10",
        "J workflow",
        "Faces showing no white of the eye",
        Apply::Blink,
        S::Any,
        &[],
    ));
    v
}

/// The photographs of `H:\AURA EDIT` where a tool has something to work on, tried first: a
/// veil over the face, white and dark garments, men with stubble, a single bride standing full
/// length. Any other folder falls back on the sorting by what the parse found.
fn preferred(id: &str) -> &'static [&'static str] {
    match id {
        "G3" => &["989A2706.CR3", "989A2705.CR3"],
        "G7" => &["989A3150.CR3", "989A2892.CR3"],
        "G8" => &["989A2933.CR3", "989A2890.CR3"],
        "D4" | "D8" => &["989A2892.CR3", "989A3842.CR3"],
        "C1" | "C2" | "C3" | "C4" | "C5" | "C6" | "C7" => &["989A2704.CR3", "989A2748.CR3"],
        "I8" => &["989A2925.CR3", "989A2699.CR3", "989A3272.CR3"],
        "G2" => &["989A3150.CR3", "989A2704.CR3", "989A2708.CR3"],
        _ => &[],
    }
}

// ---- the photographs --------------------------------------------------------------------

struct Photo {
    id: String,
    name: String,
    width: usize,
    height: usize,
    base: Vec<u8>,
    map: PortraitMap,
    /// The largest face's width as a share of the frame's short edge.
    face_share: f32,
}

impl Photo {
    fn coverage(&self, region: Region) -> f32 {
        self.map.stat(region).map_or(0.0, |s| s.coverage)
    }

    fn suits(&self, s: Suits) -> f32 {
        let faces = self.map.faces.len();
        match s {
            Suits::Portrait => {
                if faces >= 1 {
                    self.face_share
                } else {
                    -1.0
                }
            }
            Suits::Group => {
                if faces >= 3 {
                    faces as f32
                } else {
                    -1.0
                }
            }
            Suits::Beard => {
                if faces >= 1 && self.coverage(Region::FacialHair) > 0.0005 {
                    self.coverage(Region::FacialHair)
                } else {
                    -1.0
                }
            }
            Suits::BodySkin => {
                if self.coverage(Region::BodySkin) > 0.01 {
                    self.coverage(Region::BodySkin)
                } else {
                    -1.0
                }
            }
            Suits::Person => {
                if self.coverage(Region::Body) > 0.15 {
                    self.coverage(Region::Body)
                } else {
                    -1.0
                }
            }
            Suits::Sky => {
                if self.coverage(Region::Sky) > 0.03 {
                    self.coverage(Region::Sky)
                } else {
                    -1.0
                }
            }
            Suits::Any => 1.0,
        }
    }
}

struct Run {
    _dir: tempfile::TempDir,
    state: AppState,
    project: String,
}

fn open(photos: &Path) -> Run {
    let dir = tempfile::tempdir_in(
        std::env::var("AURA_EVOTO_TEMP").unwrap_or_else(|_| "F:/aura-evoto100".into()),
    )
    .expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Evoto 100".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![photos.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    Run {
        _dir: dir,
        state,
        project: project.id,
    }
}

fn decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => u32::from(c - b'A'),
        b'a'..=b'z' => u32::from(c - b'a') + 26,
        b'0'..=b'9' => u32::from(c - b'0') + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks(4) {
        let pad = chunk.iter().filter(|c| **c == b'=').count();
        let n = chunk.iter().fold(0_u32, |n, c| {
            (n << 6) | if *c == b'=' { 0 } else { value(*c) }
        });
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
    }
    out
}

fn render(run: &Run, photo: &str) -> (Vec<u8>, usize, usize, u128) {
    let started = Instant::now();
    let out = aura_app::render_image(
        &run.state,
        &RenderImageInput {
            photo_id: photo.into(),
            level: None,
            screen: None,
            colour_space: None,
            purpose: Some("export".into()),
        },
    )
    .expect("render");
    (
        decode(&out.rgb_base64),
        out.width as usize,
        out.height as usize,
        started.elapsed().as_millis(),
    )
}

fn parse(rgb: &[u8], w: usize, h: usize) -> PortraitMap {
    aura_portrait::Canvas::from_srgb8(rgb, w as u32, h as u32, aura_portrait::ANALYSIS_EDGE)
        .map_or_else(
            || PortraitMap::empty(64, 64),
            |c| aura_portrait::analyse(&c, &[]),
        )
}

// ---- applying ---------------------------------------------------------------------------

fn merge(into: &mut Value, patch: &Value) {
    match (into, patch) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

fn save_finish(run: &Run, photo: &str, f: &StudioFinish, label: &str) {
    finish_commands::save_studio_finish(
        &run.state,
        &SaveStudioFinishInput {
            project_id: run.project.clone(),
            photo_id: photo.into(),
            finish: f.clone(),
            label: Some(label.into()),
        },
    )
    .expect("save finish");
}

fn reset(run: &Run, photo: &str) {
    let ids = StudioFinishInput {
        project_id: run.project.clone(),
        photo_id: photo.into(),
    };
    let current = finish_commands::studio_finish(&run.state, &ids).expect("read");
    if current.finish != StudioFinish::default() {
        save_finish(run, photo, &StudioFinish::default(), "reset");
    }
    native_retouch::edit(
        &run.state,
        &RetouchInput {
            project_id: run.project.clone(),
            photo_id: photo.into(),
            action: "clear".into(),
            edits: vec![],
            id: None,
        },
    )
    .expect("clear");
    let pid = PhotoId::from_db(photo).expect("photo");
    let project = ProjectId::from_db(&run.project).expect("project");
    let mut recipe = aura_app::develop_commands::load_or_neutral(&run.state, pid).expect("recipe");
    if recipe.geometry != aura_recipe::Geometry::default() {
        recipe.geometry = aura_recipe::Geometry::default();
        run.state
            .recipe_store()
            .save(&project, &pid, &recipe, &["geometry".into()], "reset")
            .expect("reset geometry");
    }
}

/// An ellipse over the densest patch of a region, as a photographer would draw one over an arm
/// or a hand, normalised: `[cx, cy, rx, ry]`.
fn ellipse_over(map: &PortraitMap, region: Region) -> Option<[f32; 4]> {
    let p = map.plane(region)?;
    let (w, h) = (p.width as usize, p.height as usize);
    let side = (w.min(h) / 14).max(4);
    // An integral image, so every window's sum is four reads.
    let mut sum = vec![0.0_f64; (w + 1) * (h + 1)];
    for y in 0..h {
        for x in 0..w {
            sum[(y + 1) * (w + 1) + x + 1] = f64::from(p.values[y * w + x])
                + sum[y * (w + 1) + x + 1]
                + sum[(y + 1) * (w + 1) + x]
                - sum[y * (w + 1) + x];
        }
    }
    let mut best = (0.0_f64, 0, 0);
    for y in 0..h.saturating_sub(side) {
        for x in 0..w.saturating_sub(side) {
            let s = sum[(y + side) * (w + 1) + x + side]
                - sum[y * (w + 1) + x + side]
                - sum[(y + side) * (w + 1) + x]
                + sum[y * (w + 1) + x];
            if s > best.0 {
                best = (s, x, y);
            }
        }
    }
    (best.0 > (side * side) as f64 * 0.25).then(|| {
        let (fw, fh) = (w as f32, h as f32);
        let half = side as f32 / 2.0;
        [
            (best.1 as f32 + half) / fw,
            (best.2 as f32 + half) / fh,
            half / fw,
            half / fh,
        ]
    })
}

fn effect(tool: Tool, region: [f32; 4], amount: f32) -> Edit {
    Edit {
        id: "evoto".into(),
        tool,
        enabled: true,
        repair_passes: None,
        region,
        source: None,
        source_scale: 1.0,
        amount,
        feather: 0.3,
        radius: match tool {
            Tool::BodyHairSoften => 0.0015,
            Tool::Cellulite => 0.012,
            Tool::HandRejuvenate => 0.003,
            Tool::KnuckleEven => 0.004,
            _ => 0.01,
        },
        texture: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        sensitivity: None,
        keep_dark_marks: false,
        mole_edge: None,
        tone: if tool == Tool::TanLine { 0.2 } else { 0.5 },
        warmth: 0.0,
        tint: 0.0,
        target_color: None,
        mask: None,
        skin: None,
        selection: None,
        matte: None,
    }
}

// ---- measuring --------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
struct Measure {
    photo: String,
    changed: f32,
    mean: f32,
    inside: Option<f32>,
    ms: u128,
    note: String,
}

fn measure(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    map: &PortraitMap,
    regions: &[Region],
) -> (f32, f32, Option<f32>) {
    if before.len() != after.len() {
        return (1.0, f32::NAN, None);
    }
    let weight: Option<Vec<f32>> = (!regions.is_empty()).then(|| {
        let mut plane = vec![0.0_f32; w * h];
        for r in regions {
            let p = map.resolve(*r, w as u32, h as u32);
            for (slot, v) in plane.iter_mut().zip(&p.values) {
                *slot = slot.max(*v);
            }
        }
        // A little room for the feathered edges every region has.
        let dilated = aura_render::bands::blur(&plane, w, h, (w.max(h) / 200).max(2));
        plane
            .iter()
            .zip(&dilated)
            .map(|(a, b)| a.max((b * 3.0).min(1.0)))
            .collect()
    });
    let (mut changed, mut total, mut inside) = (0_usize, 0.0_f64, 0.0_f64);
    for i in 0..w * h {
        let d: i32 = (0..3)
            .map(|c| (i32::from(before[i * 3 + c]) - i32::from(after[i * 3 + c])).abs())
            .max()
            .unwrap_or(0);
        if d >= 3 {
            changed += 1;
        }
        let sum: f64 = (0..3)
            .map(|c| f64::from((i32::from(before[i * 3 + c]) - i32::from(after[i * 3 + c])).abs()))
            .sum();
        total += sum;
        if let Some(wt) = &weight {
            inside += sum * f64::from(wt[i]);
        }
    }
    let n = (w * h) as f32;
    (
        changed as f32 / n,
        (total / (3.0 * f64::from(n))) as f32,
        weight.map(|_| {
            if total > 0.0 {
                (inside / total) as f32
            } else {
                1.0
            }
        }),
    )
}

/// The box a crop of this tool's effect is cut from: the feature a face tool works on, or the
/// change's own extent otherwise.
fn crop_box(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    map: &PortraitMap,
    id: &str,
) -> [usize; 4] {
    let focus = match id {
        "A6" | "A7" | "A8" | "A9" => Some((0.6, 0.6, -0.1, 1.15)),
        "B1" | "B2" | "B3" | "D2" | "D6" | "E5" | "E6" => Some((0.75, 0.75, 0.75, 1.75)),
        "A1" | "A2" | "A3" | "A4" | "A5" | "B4" | "B5" | "B6" | "D1" | "D7" | "E1" | "E2"
        | "E3" | "E4" | "E10" | "G10" => Some((1.15, 1.15, -0.75, 0.6)),
        "B10" | "F1" | "F2" | "F3" | "F4" | "C10" => Some((2.3, 2.3, -2.9, 2.6)),
        id if matches!(id.chars().next(), Some('A' | 'B' | 'D' | 'E'))
            || matches!(id, "C8" | "C9" | "I7" | "I10" | "G3") =>
        {
            Some((1.5, 1.5, -1.4, 2.6))
        }
        _ => None,
    };
    if let (Some((l, r, t, b)), Some(f)) = (focus, map.faces.first()) {
        let s = w as f32 / map.width.max(1) as f32;
        let io = f.interocular() * s;
        let (mx, my) = (
            (f.left_eye[0] + f.right_eye[0]) * 0.5 * s,
            (f.left_eye[1] + f.right_eye[1]) * 0.5 * s,
        );
        let clamp = |v: f32, hi: usize| (v.max(0.0) as usize).min(hi);
        let bx = [
            clamp(mx - l * io, w),
            clamp(my + t * io, h),
            clamp(mx + r * io, w),
            clamp(my + b * io, h),
        ];
        if bx[2] > bx[0] + 8 && bx[3] > bx[1] + 8 {
            return bx;
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 3;
            if (0..3).any(|c| before[i + c].abs_diff(after[i + c]) >= 6) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 {
        return [0, 0, w, h];
    }
    let pad = (w.max(h) / 40).max(8);
    [
        x0.saturating_sub(pad),
        y0.saturating_sub(pad),
        (x1 + pad).min(w),
        (y1 + pad).min(h),
    ]
}

/// Before, after, and where it changed - the difference eight times over, so a warp of one
/// pixel shows - side by side, enlarged until the crop is at least 360 pixels tall.
fn write_pair(path: &Path, before: &[u8], after: &[u8], w: usize, b: [usize; 4]) {
    let (cw, ch) = (b[2] - b[0], b[3] - b[1]);
    let gap = 4;
    let out_w = cw * 3 + gap * 2;
    let mut out = vec![255_u8; out_w * ch * 3];
    for y in 0..ch {
        for x in 0..cw {
            let src = ((b[1] + y) * w + b[0] + x) * 3;
            let row = y * out_w;
            out[(row + x) * 3..(row + x) * 3 + 3].copy_from_slice(&before[src..src + 3]);
            let dst = (row + cw + gap + x) * 3;
            out[dst..dst + 3].copy_from_slice(&after[src..src + 3]);
            let diff = (row + 2 * (cw + gap) + x) * 3;
            for c in 0..3 {
                out[diff + c] =
                    (u32::from(before[src + c].abs_diff(after[src + c])) * 8).min(255) as u8;
            }
        }
    }
    let img = image::RgbImage::from_raw(out_w as u32, ch as u32, out).expect("image");
    let scale = (360.0 / ch as f32)
        .max(1.0)
        .min(1500.0 / out_w as f32)
        .max(0.2);
    let img = if (scale - 1.0).abs() > 0.01 {
        image::imageops::resize(
            &img,
            (out_w as f32 * scale) as u32,
            (ch as f32 * scale).max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    img.save(path).expect("crop");
}

/// The parse drawn over the photograph, one colour per region, for checking what the tools are
/// anchored on: lips red, mouth magenta, eyes cyan, brows blue, skin faint green, hair yellow,
/// clothing orange, face boxes and landmarks white.
fn dump_parse(path: &Path, rgb: &[u8], w: usize, h: usize, map: &PortraitMap) {
    let mut out = rgb.to_vec();
    let paint = |out: &mut Vec<u8>, region: Region, colour: [u8; 3], strength: f32| {
        let plane = map.resolve(region, w as u32, h as u32);
        for (i, v) in plane.values.iter().enumerate() {
            let k = v.clamp(0.0, 1.0) * strength;
            for c in 0..3 {
                let o = &mut out[i * 3 + c];
                *o = (f32::from(*o) * (1.0 - k) + f32::from(colour[c]) * k) as u8;
            }
        }
    };
    paint(&mut out, Region::Skin, [0, 255, 0], 0.25);
    paint(&mut out, Region::Hair, [255, 255, 0], 0.35);
    paint(&mut out, Region::Clothing, [255, 140, 0], 0.25);
    paint(&mut out, Region::Eyebrows, [0, 0, 255], 0.7);
    paint(&mut out, Region::Eyes, [0, 255, 255], 0.7);
    paint(&mut out, Region::Sclera, [255, 255, 255], 0.9);
    paint(&mut out, Region::Mouth, [255, 0, 255], 0.5);
    paint(&mut out, Region::Lips, [255, 0, 0], 0.7);
    let s = w as f32 / map.width.max(1) as f32;
    for f in &map.faces {
        for p in [f.left_eye, f.right_eye, f.nose, f.mouth] {
            let (cx, cy) = ((p[0] * s) as i64, (p[1] * s) as i64);
            for dy in -3..=3_i64 {
                for dx in -3..=3_i64 {
                    let (x, y) = (cx + dx, cy + dy);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        let i = (y as usize * w + x as usize) * 3;
                        out[i..i + 3].copy_from_slice(&[255, 255, 255]);
                    }
                }
            }
        }
    }
    let img = image::RgbImage::from_raw(w as u32, h as u32, out).expect("img");
    img.save(path).expect("parse");
    let plain = image::RgbImage::from_raw(w as u32, h as u32, rgb.to_vec()).expect("plain");
    plain.save(path.with_extension("plain.jpg")).expect("plain");
    // One crop per face, for checking every face's landmarks at a glance.
    let faces_dir = path.with_extension("faces");
    std::fs::create_dir_all(&faces_dir).expect("faces dir");
    for (k, f) in map.faces.iter().enumerate() {
        let half = f.size * s * 1.1;
        let (cx, cy) = (f.center[0] * s, f.center[1] * s);
        let x0 = (cx - half).max(0.0) as u32;
        let y0 = (cy - half * 0.9).max(0.0) as u32;
        let x1 = ((cx + half) as u32).min(w as u32);
        let y1 = ((cy + half * 1.3) as u32).min(h as u32);
        if x1 > x0 + 8 && y1 > y0 + 8 {
            let crop = image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image();
            let crop = image::imageops::resize(
                &crop,
                220,
                (220 * (y1 - y0) / (x1 - x0)).max(1),
                image::imageops::FilterType::Triangle,
            );
            crop.save(faces_dir.join(format!("{k}.jpg"))).expect("face");
            let bare = image::imageops::crop_imm(&plain, x0, y0, x1 - x0, y1 - y0).to_image();
            let bare = image::imageops::resize(
                &bare,
                440,
                (440 * (y1 - y0) / (x1 - x0)).max(1),
                image::imageops::FilterType::Lanczos3,
            );
            bare.save(faces_dir.join(format!("{k}_plain.jpg")))
                .expect("face");
        }
    }
}

// ---- the run ----------------------------------------------------------------------------

#[test]
#[ignore = "needs AURA_EVOTO_PHOTOS and AURA_EVOTO_OUT"]
fn every_tool_on_real_photographs() {
    let Ok(source) = std::env::var("AURA_EVOTO_PHOTOS") else {
        println!("AURA_EVOTO_PHOTOS is not set");
        return;
    };
    let out = PathBuf::from(std::env::var("AURA_EVOTO_OUT").expect("AURA_EVOTO_OUT"));
    std::fs::create_dir_all(out.join("crops")).expect("out");
    std::fs::create_dir_all(out.join("exports")).expect("exports");
    let per_tool: usize = std::env::var("AURA_EVOTO_PER_TOOL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let only: Vec<String> = std::env::var("AURA_EVOTO_ONLY")
        .map(|v| v.split(',').map(str::to_owned).collect())
        .unwrap_or_default();

    let started = Instant::now();
    let run = open(Path::new(&source));
    let rows = aura_app::list_images(
        &run.state,
        &ListImagesInput {
            project_id: run.project.clone(),
            offset: 0,
            limit: 500,
            order_by: None,
        },
    )
    .expect("photos");
    println!(
        "imported {} photographs in {:.1}s",
        rows.len(),
        started.elapsed().as_secs_f32()
    );

    let mut photos = Vec::new();
    for row in &rows {
        let (base, w, h, ms) = render(&run, &row.id);
        // The renderer's own parse - of the frame as decoded - which every tool reads.
        let photo = PhotoId::from_db(&row.id).expect("photo id");
        let recipe =
            aura_app::develop_commands::load_or_neutral(&run.state, photo).expect("recipe");
        let (frame, parsed) = evoto_workflow::parse_of(&run.state, photo, &recipe).expect("parse");
        let map = parsed.map_or_else(|| parse(&base, w, h), |m| (*m).clone());
        if std::env::var_os("AURA_EVOTO_DUMP_PARSE").is_some() {
            // The pixels the parse itself saw: the decoded frame, downsampled and encoded.
            if let Some(c) = aura_portrait::Canvas::from_linear(
                &frame.rgb,
                frame.width,
                frame.height,
                aura_portrait::canvas::Primaries::Rec2020,
                aura_portrait::ANALYSIS_EDGE,
            ) {
                let bytes: Vec<u8> = c
                    .srgb
                    .iter()
                    .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                    .collect();
                std::fs::create_dir_all(out.join("parse")).expect("parse dir");
                image::RgbImage::from_raw(c.width, c.height, bytes)
                    .expect("canvas")
                    .save(
                        out.join("parse")
                            .join(format!("{}.canvas.png", row.file_name)),
                    )
                    .expect("canvas");
            }
        }
        let face_share = map
            .faces
            .first()
            .map_or(0.0, |f| f.size / map.width.min(map.height).max(1) as f32);
        println!("{:<14} {}x{} {:>5} ms  faces {:>2}  main face {:.2}  beard {:.4}  body skin {:.3}  sky {:.3}", row.file_name, w, h, ms, map.faces.len(), face_share,
            map.stat(Region::FacialHair).map_or(0.0, |s| s.coverage), map.stat(Region::BodySkin).map_or(0.0, |s| s.coverage), map.stat(Region::Sky).map_or(0.0, |s| s.coverage));
        if std::env::var_os("AURA_EVOTO_DUMP_PARSE").is_some() {
            std::fs::create_dir_all(out.join("parse")).expect("parse dir");
            dump_parse(
                &out.join("parse").join(format!("{}.jpg", row.file_name)),
                &base,
                w,
                h,
                &map,
            );
        }
        photos.push(Photo {
            id: row.id.clone(),
            name: row.file_name.clone(),
            width: w,
            height: h,
            base,
            map,
            face_share,
        });
    }

    let mut results = Vec::new();
    for c in catalogue() {
        if !only.is_empty() && !only.iter().any(|o| o == c.id) {
            continue;
        }
        let mut ranked: Vec<&Photo> = photos.iter().filter(|p| p.suits(c.suits) >= 0.0).collect();
        let wanted = preferred(c.id);
        let first = |p: &Photo| {
            wanted
                .iter()
                .position(|n| p.name == *n)
                .unwrap_or(usize::MAX)
        };
        ranked.sort_by(|a, b| {
            first(a)
                .cmp(&first(b))
                .then(b.suits(c.suits).total_cmp(&a.suits(c.suits)))
                .then(a.name.cmp(&b.name))
        });
        let take = if matches!(c.apply, Apply::Export(..)) {
            per_tool.min(1)
        } else {
            per_tool
        };
        let chosen: Vec<&Photo> = ranked.into_iter().take(take).collect();
        let mut measures = Vec::new();
        if matches!(c.apply, Apply::Blink) {
            let t = Instant::now();
            let report = evoto_workflow::blink_check(
                &run.state,
                &BlinkInput {
                    project_id: run.project.clone(),
                    photo_ids: None,
                },
            )
            .expect("blink");
            let names: Vec<String> = report
                .flagged
                .iter()
                .map(|f| {
                    let name = photos
                        .iter()
                        .find(|p| p.id == f.photo_id)
                        .map_or("?", |p| p.name.as_str());
                    format!(
                        "{name} ({} of {} faces showing no white of the eye)",
                        f.closed,
                        f.faces.len()
                    )
                })
                .collect();
            measures.push(Measure {
                photo: "collection".into(),
                changed: 0.0,
                mean: 0.0,
                inside: None,
                ms: t.elapsed().as_millis(),
                note: format!(
                    "checked {}, flagged {}: {}; without a face {}",
                    report.checked,
                    report.flagged.len(),
                    names.join(", "),
                    report.without_faces
                ),
            });
            println!("{} {}: {}", c.id, c.label, measures[0].note);
            results.push(
                json!({ "id": c.id, "group": c.group, "label": c.label, "measures": measures }),
            );
            continue;
        }
        for (k, p) in chosen.iter().enumerate() {
            reset(&run, &p.id);
            let mut note = String::new();
            let t = Instant::now();
            match &c.apply {
                Apply::Finish(patch) => {
                    let mut value = serde_json::to_value(StudioFinish::default()).expect("json");
                    merge(&mut value, patch);
                    let f: StudioFinish = serde_json::from_value(value).expect("finish");
                    save_finish(&run, &p.id, &f, c.label);
                }
                Apply::Effect(tool, region, amount) => {
                    let Some(r) = ellipse_over(&p.map, *region) else {
                        note = format!("no {} found to place it on", region.as_str());
                        measures.push(Measure {
                            photo: p.name.clone(),
                            changed: 0.0,
                            mean: 0.0,
                            inside: None,
                            ms: 0,
                            note,
                        });
                        continue;
                    };
                    native_retouch::edit(
                        &run.state,
                        &RetouchInput {
                            project_id: run.project.clone(),
                            photo_id: p.id.clone(),
                            action: "append".into(),
                            edits: vec![effect(*tool, r, *amount)],
                            id: None,
                        },
                    )
                    .expect("effect");
                }
                Apply::FacePicker => {
                    // Glow and rosy on the second-largest face only.
                    let g = &p.map.faces[1];
                    let centre = [
                        g.center[0] / p.map.width as f32,
                        g.center[1] / p.map.height as f32,
                    ];
                    let f = StudioFinish {
                        faces: vec![centre],
                        skin: aura_recipe::studio_finish::SkinFinish {
                            glow: 100.0,
                            rosy: 100.0,
                            ..Default::default()
                        },
                        subject: aura_recipe::studio_finish::SubjectFinish {
                            face_brightness: 60.0,
                            ..Default::default()
                        },
                        ..StudioFinish::default()
                    };
                    save_finish(&run, &p.id, &f, "face picker");
                    note = format!(
                        "chosen face at {:.2},{:.2} of {}",
                        centre[0],
                        centre[1],
                        p.map.faces.len()
                    );
                }
                Apply::Framing(preset) => {
                    let made = evoto_workflow::apply_evoto_framing(
                        &run.state,
                        &FramingInput {
                            project_id: run.project.clone(),
                            photo_id: p.id.clone(),
                            preset: (*preset).into(),
                            face: None,
                            level: false,
                        },
                    )
                    .expect("framing");
                    let (cw, ch) = (
                        (made.crop[2] - made.crop[0]) * p.width as f32,
                        (made.crop[3] - made.crop[1]) * p.height as f32,
                    );
                    note = format!("crop {:.3},{:.3},{:.3},{:.3} ({:.0}x{:.0} at {:.3}:1), rotate {:.2} deg, white {}; {}", made.crop[0], made.crop[1], made.crop[2], made.crop[3], cw, ch, cw / ch, made.rotate, made.white_background, made.reasons.join(" "));
                }
                Apply::Export(kind, count) => {
                    let folder = out.join("exports").join(c.id);
                    let made = evoto_workflow::evoto_export(
                        &run.state,
                        &ExportInput {
                            project_id: run.project.clone(),
                            photo_id: p.id.clone(),
                            kind: (*kind).into(),
                            folder: folder.display().to_string(),
                            count: *count,
                        },
                    );
                    match made {
                        Ok(m) => {
                            note = format!(
                                "{} files, {}x{}; {}; {}",
                                m.files.len(),
                                m.width,
                                m.height,
                                m.reasons.join(" "),
                                m.files
                                    .iter()
                                    .map(|f| Path::new(f).file_name().map_or(String::new(), |n| n
                                        .to_string_lossy()
                                        .into_owned()))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                        Err(e) => note = format!("FAILED: {e:?}"),
                    }
                    measures.push(Measure {
                        photo: p.name.clone(),
                        changed: 0.0,
                        mean: 0.0,
                        inside: None,
                        ms: t.elapsed().as_millis(),
                        note: note.clone(),
                    });
                    println!("{} {} on {}: {}", c.id, c.label, p.name, note);
                    continue;
                }
                Apply::Blink => {}
            }
            let (after, w, h, ms) = render(&run, &p.id);
            let (changed, mean, inside) = if (w, h) == (p.width, p.height) {
                measure(&p.base, &after, w, h, &p.map, &c.regions)
            } else {
                (1.0, f32::NAN, None)
            };
            if k == 0 {
                let path = out.join("crops").join(format!("{}.jpg", c.id));
                if (w, h) == (p.width, p.height) {
                    let b = crop_box(&p.base, &after, w, h, &p.map, c.id);
                    write_pair(&path, &p.base, &after, w, b);
                } else {
                    // A crop: the whole frame before, the framed result after, side by side.
                    let left =
                        image::RgbImage::from_raw(p.width as u32, p.height as u32, p.base.clone())
                            .expect("img");
                    let right =
                        image::RgbImage::from_raw(w as u32, h as u32, after.clone()).expect("img");
                    let hh = 600_u32;
                    let l = image::imageops::resize(
                        &left,
                        (p.width as f32 * hh as f32 / p.height as f32) as u32,
                        hh,
                        image::imageops::FilterType::Triangle,
                    );
                    let r = image::imageops::resize(
                        &right,
                        (w as f32 * hh as f32 / h as f32) as u32,
                        hh,
                        image::imageops::FilterType::Triangle,
                    );
                    let mut both = image::RgbImage::from_pixel(
                        l.width() + r.width() + 6,
                        hh,
                        image::Rgb([255, 255, 255]),
                    );
                    image::imageops::replace(&mut both, &l, 0, 0);
                    image::imageops::replace(&mut both, &r, i64::from(l.width() + 6), 0);
                    both.save(&path).expect("crop");
                }
            }
            println!(
                "{:<4} {:<28} {:<14} changed {:>6.3}  mean {:>6.2}  inside {:>6}  {:>5} ms  {}",
                c.id,
                c.label,
                p.name,
                changed,
                mean,
                inside.map_or("-".into(), |v| format!("{v:.3}")),
                ms,
                note
            );
            measures.push(Measure {
                photo: p.name.clone(),
                changed,
                mean,
                inside,
                ms,
                note,
            });
            let _ = t;
        }
        if chosen.is_empty() {
            println!("{} {}: no photograph suits it", c.id, c.label);
        }
        results
            .push(json!({ "id": c.id, "group": c.group, "label": c.label, "measures": measures }));
    }
    for p in &photos {
        reset(&run, &p.id);
    }
    let faces: Vec<Value> = photos.iter().map(|p| json!({
        "photo": p.name, "faces": p.map.faces.len(), "mainFace": p.face_share,
        "eyes": p.map.faces.iter().map(|g| evoto_workflow::eye_openness(&p.map, g)).collect::<Vec<_>>(),
        "mapWidth": p.map.width,
        "geometry": p.map.faces.iter().map(|g| json!({
            "leftEye": g.left_eye, "rightEye": g.right_eye, "mouth": g.mouth, "mouthWidth": g.mouth_width, "size": g.size,
            "center": g.center, "roll": g.roll, "eyesMeasured": g.eyes_measured,
        })).collect::<Vec<_>>(),
    })).collect();
    std::fs::write(
        out.join("results.json"),
        serde_json::to_string_pretty(
            &json!({ "tools": results, "photos": faces, "seconds": started.elapsed().as_secs() }),
        )
        .expect("json"),
    )
    .expect("write");
    println!("done in {:.0} s", started.elapsed().as_secs_f32());
}
