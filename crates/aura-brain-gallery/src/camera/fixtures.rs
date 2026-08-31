//! The synthetic multi-camera weddings every section 10.1 gate is measured against.
//!
//! ## What these prove and what they do not
//!
//! A fixture here is built by **choosing** a per-brand colour departure, applying it to authored
//! readings, and handing the result to the real fingerprint, the real pairing, the real solver and
//! the real held-out check. When the solver recovers the departure that was applied, what has been
//! proved is that the pipeline is arithmetically correct end to end.
//!
//! What has **not** been proved is anything about a wedding. There are no multi-camera weddings in
//! this repository, no measured body and no photographed target: section 9's DATA row asks for
//! Sony+Canon, Canon+Nikon and Fujifilm fixtures with matched scenes and there are none. That is
//! condition C1 of `docs/progress/PHASE-26-EXIT.md`, it is a Sev 2 trigger, and it closes with
//! phase 05's C10 rather than separately - the pairing pre-filter reads the placeholder embedding
//! and the skin term reads a field this build cannot fill.
//!
//! The fixtures are deliberately built to be **recoverable but not trivially so**: every frame
//! carries honest per-frame jitter, the two bodies photograph the same rooms at slightly different
//! moments, and a share of the candidate pairs are of genuinely different rooms so the background
//! verification has something to reject. A fixture whose answer falls out of one subtraction proves
//! that the subtraction works.

use std::collections::BTreeMap;

use aura_core::contract::camera::{Brand, FlashState};
use aura_core::contract::gallery::ImageId;
use aura_core::contract::ids::NodeId;
use aura_core::contract::moment::CameraId;
use aura_core::SceneId;

use crate::tree::Frame;

use super::fingerprint::{BackgroundStats, CameraFrame};

/// One camera in a synthetic wedding, and how it renders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    /// The catalog camera id this body is given.
    pub id: &'static str,
    /// The shooter label.
    pub shooter: &'static str,
    /// The manufacturer.
    pub brand: Brand,
    /// How far off the reference this body's temperature runs, in kelvin.
    pub d_cct: f32,
    /// How far off in tint.
    pub d_tint: f32,
    /// How far off in subject luminance, as a multiplier: 0.8 is a body a quarter-stop darker.
    pub luma_scale: f32,
    /// How far off in saturation, in the recipe's units.
    pub d_saturation: f32,
}

impl Body {
    /// The reference body: renders exactly on the nominal.
    pub const REFERENCE: Self = Self {
        id: "cam_lead",
        shooter: "primary",
        brand: Brand::Canon,
        d_cct: 0.0,
        d_tint: 0.0,
        luma_scale: 1.0,
        d_saturation: 0.0,
    };

    /// A second body: cooler, greener, and carried by somebody who exposes darker.
    pub const SECOND: Self = Self {
        id: "cam_second",
        shooter: "second",
        brand: Brand::Sony,
        d_cct: -420.0,
        d_tint: -5.0,
        luma_scale: 0.80,
        d_saturation: -3.0,
    };

    /// A third body: warmer and more contrasty, carried by the same person as the reference.
    pub const THIRD: Self = Self {
        id: "cam_third",
        shooter: "primary",
        brand: Brand::Fujifilm,
        d_cct: 260.0,
        d_tint: 2.0,
        luma_scale: 1.0,
        d_saturation: 4.0,
    };
}

/// How a synthetic wedding is shaped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// How many scene nodes it has.
    pub nodes: usize,
    /// How many frames each body shoots in each node.
    pub per_node: usize,
    /// How far apart the two bodies' frames of the same moment are, in milliseconds.
    pub offset_ms: i64,
    /// How many of the nodes are lit differently from each other.
    ///
    /// Every node above this many shares a room with node zero, which is what gives the background
    /// verification something to confuse itself with if it were reading subjects rather than rooms.
    pub distinct_rooms: usize,
    /// Which scene the nodes are.
    pub scene: SceneId,
    /// Whether the frames carry a flash.
    pub flash: FlashState,
    /// Whether the frames carry a skin reading.
    ///
    /// False is this build on a real photograph - `SKIN_FIELD_AVAILABLE` is false - and the gates
    /// that measure the skin promise set it true against **authored readings**, which proves the
    /// arithmetic and says nothing about a person. Phase 25's condition C2, inherited.
    pub with_skin: bool,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            nodes: 4,
            per_node: 12,
            offset_ms: 8_000,
            distinct_rooms: 3,
            scene: SceneId::Ceremony,
            flash: FlashState::Ambient,
            with_skin: true,
        }
    }
}

/// The nominal temperature of the reference body's light in each room, in kelvin.
const ROOM_CCT: [f32; 4] = [5200.0, 3200.0, 6400.0, 4400.0];

/// A whole synthetic wedding, shot by a list of bodies.
///
/// Frames come back in capture order across all bodies, which is how the catalog would hand them
/// over and is what makes the pairing's node index do real work.
#[must_use]
pub fn wedding(bodies: &[Body], shape: Shape) -> Vec<CameraFrame> {
    let nodes: Vec<NodeId> = (0..shape.nodes).map(|_| NodeId::new()).collect();
    let mut frames = Vec::new();

    for (node_index, node) in nodes.iter().enumerate() {
        let room = node_index.min(shape.distinct_rooms.saturating_sub(1));
        let nominal = ROOM_CCT
            .get(room % ROOM_CCT.len())
            .copied()
            .unwrap_or(5200.0);
        let (hist, luma) = room_look(room);

        for index in 0..shape.per_node {
            #[allow(clippy::cast_precision_loss)]
            let jitter = ((index % 5) as f32 - 2.0) * 18.0;
            let base_ms = (node_index as i64) * 1_800_000 + (index as i64) * 4_000;

            for (body_index, body) in bodies.iter().enumerate() {
                let ms = base_ms + (body_index as i64) * shape.offset_ms;
                frames.push(frame_for(
                    body,
                    *node,
                    shape,
                    ms,
                    nominal + jitter,
                    &hist,
                    luma,
                ));
            }
        }
    }

    frames.sort_by_key(|frame| frame.timeline_ms);
    frames
}

/// One frame from one body.
fn frame_for(
    body: &Body,
    node: NodeId,
    shape: Shape,
    timeline_ms: i64,
    nominal_cct: f32,
    // Borrowed rather than copied: a 512-byte histogram by value on a function called once per
    // frame per body is half a kilobyte of memcpy per fixture frame, and a fixture that is slow for
    // no reason makes every gate slower.
    hist: &[u8; 512],
    luma: [f32; 4],
) -> CameraFrame {
    use aura_raw::colour::illuminant::cct_to_uv;

    let cct = nominal_cct + body.d_cct;
    let white = {
        let on_locus = cct_to_uv(cct);
        // Tint moves perpendicular to the locus, which is what the axis means. The same relation
        // `transform::shift_uv` uses, so a fixture and the code under test agree about what a tint
        // unit is - without that they would be measuring two different quantities.
        [on_locus[0], on_locus[1] + body.d_tint * 0.0005]
    };
    let subject_luma = (0.45 * body.luma_scale).clamp(0.0, 1.0);
    let skin = shape.with_skin.then(|| {
        // Skin sits off the illuminant by a fixed offset, so a body that renders the light warm
        // renders skin warm by the same amount. That is the relation a camera transform exists to
        // correct and it is authored here rather than assumed by the solver.
        //
        // The offset is small - about 0.015 in `u'v'` - because that is roughly where skin sits
        // relative to the light it is under. An exaggerated offset would put the fixture's skin far
        // off the Planckian locus, where the locus walk `transform::shift_uv` performs is least
        // well conditioned, and the gate would then be measuring the conditioning rather than the
        // solver.
        [white[0] + 0.014, white[1] + 0.006]
    });

    CameraFrame {
        image: ImageId::new(),
        camera: CameraId::new(body.id),
        brand: body.brand,
        shooter: body.shooter.to_string(),
        flash: shape.flash,
        node: Some(node),
        scene: shape.scene,
        timeline_ms,
        cct_k: Some(cct),
        tint: Some(body.d_tint),
        exposure_ev: Some(0.0),
        subject_luma: Some(subject_luma),
        wb_conf: 0.82,
        white_uv: Some(white),
        skin_uv: skin,
        skin_luma: shape.with_skin.then_some(subject_luma),
        contrast: Some(10.0),
        saturation: Some(6.0 + body.d_saturation),
        signature: Some(signature_for(body)),
        embedding: Some(vec![1.0, 0.15, 0.05, 0.0]),
        background: Some(BackgroundStats::from_descriptors(hist, luma, 0.22)),
    }
}

/// The eight-number grade character a body renders with.
fn signature_for(body: &Body) -> [f32; 8] {
    let sat = (0.30 + body.d_saturation / 100.0).clamp(0.0, 1.0);
    [
        body.d_tint / 400.0,
        0.05,
        body.d_tint / 400.0,
        0.05,
        sat,
        sat,
        0.10,
        0.20,
    ]
}

/// What one room looks like: a histogram and four luminance percentiles.
///
/// Rooms differ in **hue distribution and brightness** rather than in the subject, which is what
/// makes a pair from two different rooms rejectable by a verifier that reads backgrounds and
/// invisible to one that reads subjects. The fixture is built to be able to catch that mistake.
fn room_look(room: usize) -> ([u8; 512], [f32; 4]) {
    let mut hist = [0_u8; 512];
    let hue_band = room % 8;
    for h in 0..8_usize {
        let weight: u8 = if h == hue_band {
            180
        } else if h == (hue_band + 1) % 8 {
            60
        } else {
            6
        };
        for s in 0..8_usize {
            for v in 0..8_usize {
                if let Some(slot) = hist.get_mut(h * 64 + s * 8 + v) {
                    *slot = weight / u8::try_from(1 + s.abs_diff(3) + v.abs_diff(4)).unwrap_or(1);
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let brightness = 0.42 - (room as f32) * 0.10;
    (
        hist,
        [
            brightness,
            (brightness - 0.34).max(0.0),
            brightness - 0.02,
            (brightness + 0.50).min(1.0),
        ],
    )
}

/// One ordinary frame, for a unit test that needs a `CameraFrame` and does not care what is in it.
#[must_use]
pub fn plain_frame(camera: &str) -> CameraFrame {
    let (hist, luma) = room_look(0);
    CameraFrame {
        image: ImageId::new(),
        camera: CameraId::new(camera),
        brand: Brand::Canon,
        shooter: "primary".to_string(),
        flash: FlashState::Ambient,
        node: Some(NodeId::new()),
        scene: SceneId::Ceremony,
        timeline_ms: 0,
        cct_k: Some(5200.0),
        tint: Some(0.0),
        exposure_ev: Some(0.0),
        subject_luma: Some(0.45),
        wb_conf: 0.8,
        white_uv: Some([0.20, 0.47]),
        skin_uv: Some([0.23, 0.50]),
        skin_luma: Some(0.45),
        contrast: Some(10.0),
        saturation: Some(6.0),
        signature: Some([0.10; 8]),
        embedding: Some(vec![1.0, 0.15, 0.05, 0.0]),
        background: Some(BackgroundStats::from_descriptors(&hist, luma, 0.22)),
    }
}

/// The same frame as a phase 25 gallery frame, for the ordering test.
///
/// The two carry the same numbers so a test can apply a transform through both hooks and assert
/// they agree. A frame that reached phase 25 with a different temperature from the one the solver
/// measured would make every node target subtly wrong, in a way no per-frame gate would catch.
#[must_use]
pub fn plain_gallery_frame(image: ImageId) -> Frame {
    Frame {
        image,
        segment: aura_core::SegmentId::new(),
        scene: SceneId::Ceremony,
        timeline_ms: 0,
        cct_k: Some(5200.0),
        tint: Some(0.0),
        exposure_ev: Some(0.0),
        subject_luma: Some(0.45),
        wb_conf: 0.8,
        exposure_conf: 0.8,
        mixed_light: false,
        intentional_light: false,
        mood: 0.0,
        contrast: Some(10.0),
        saturation: Some(6.0),
        signature: Some([0.10; 8]),
        identities: BTreeMap::new(),
        user_edited: false,
        enabled: true,
    }
}

/// A wedding where the second body never photographs the same room as the reference.
///
/// Section 10.1's fallback gate: "with no matched pairs, brand baselines are used and the report
/// says so honestly". The two bodies work the whole day and never overlap, which is a real wedding
/// - one photographer with the bride and one with the groom - and not a contrived one.
#[must_use]
pub fn wedding_with_no_overlap(bodies: &[Body], shape: Shape) -> Vec<CameraFrame> {
    let mut frames = wedding(bodies, shape);
    // Give every body its own nodes. Two frames in different nodes were shot under different light
    // by construction, so no pair can form - which is exactly what a wedding shot in two rooms is.
    let mut per_body: BTreeMap<String, NodeId> = BTreeMap::new();
    for frame in &mut frames {
        let node = *per_body
            .entry(frame.camera.as_str().to_string())
            .or_default();
        frame.node = Some(node);
    }
    frames
}

/// Which body a fixture wedding's frames belong to, for building a transform field.
#[must_use]
pub fn image_bodies(frames: &[CameraFrame]) -> Vec<(ImageId, CameraId, FlashState)> {
    frames
        .iter()
        .map(|frame| (frame.image, frame.camera.clone(), frame.flash))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_camera_wedding_is_shaped_the_way_the_shape_says() {
        let shape = Shape::default();
        let frames = wedding(&[Body::REFERENCE, Body::SECOND], shape);
        assert_eq!(frames.len(), shape.nodes * shape.per_node * 2);
        let lead = frames
            .iter()
            .filter(|f| f.camera.as_str() == Body::REFERENCE.id)
            .count();
        assert_eq!(lead, shape.nodes * shape.per_node);
        assert!(frames
            .windows(2)
            .all(|w| w[0].timeline_ms <= w[1].timeline_ms));
    }

    #[test]
    fn the_two_bodies_really_do_render_differently() {
        let frames = wedding(&[Body::REFERENCE, Body::SECOND], Shape::default());
        let mean = |id: &str| -> f32 {
            let values: Vec<f32> = frames
                .iter()
                .filter(|f| f.camera.as_str() == id)
                .filter_map(|f| f.cct_k)
                .collect();
            values.iter().sum::<f32>() / values.len().max(1) as f32
        };
        let gap = mean(Body::REFERENCE.id) - mean(Body::SECOND.id);
        assert!(
            (gap - 420.0).abs() < 1.0,
            "authored gap not recovered: {gap}"
        );
    }

    #[test]
    fn two_rooms_disagree_and_one_room_agrees_with_itself() {
        let (hist_a, luma_a) = room_look(0);
        let (hist_b, luma_b) = room_look(1);
        let a = BackgroundStats::from_descriptors(&hist_a, luma_a, 0.22);
        let b = BackgroundStats::from_descriptors(&hist_b, luma_b, 0.22);
        assert!(a.agreement(&a) > 0.9);
        assert!(a.agreement(&b) < 0.5, "{}", a.agreement(&b));
    }

    #[test]
    fn a_no_overlap_wedding_puts_the_two_bodies_in_different_nodes() {
        let frames = wedding_with_no_overlap(&[Body::REFERENCE, Body::SECOND], Shape::default());
        let lead_node = frames
            .iter()
            .find(|f| f.camera.as_str() == Body::REFERENCE.id)
            .and_then(|f| f.node);
        let second_node = frames
            .iter()
            .find(|f| f.camera.as_str() == Body::SECOND.id)
            .and_then(|f| f.node);
        assert!(lead_node.is_some() && second_node.is_some());
        assert_ne!(lead_node, second_node);
    }

    #[test]
    fn a_shape_without_skin_carries_no_skin_reading_anywhere() {
        // This build on a real photograph. The gates that measure the skin promise set it true
        // against authored readings and say so.
        let shape = Shape {
            with_skin: false,
            ..Shape::default()
        };
        let frames = wedding(&[Body::REFERENCE, Body::SECOND], shape);
        assert!(frames.iter().all(|f| f.skin_uv.is_none()));
    }
}
