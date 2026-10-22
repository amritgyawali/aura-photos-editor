//! Mechanical coverage, pixel provenance, limits and history checks for all 25 new workflows.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]
use super::*;
use crate::mask_workflows::{advanced::Options, advanced_pixels::Frame};

fn options() -> Options {
    Options::default()
}
fn topology(source: &[u8], width: usize, action: Action, o: &Options) -> Vec<u8> {
    let mut p = plane(source, width);
    p.topology(action, o, &CancelToken::new())
        .expect("topology");
    p.alpha
}
fn geometry(action: Action, o: &Options) -> Plane {
    let mut p = plane(&[0; 400], 20);
    p.geometry(action, o, None, &CancelToken::new())
        .expect("shape");
    p
}
#[test]
fn authored_shapes_rasterize_actual_coverage_and_validate_perspective_corners() {
    let o = options();
    let polygon = geometry(Action::PolygonMarquee, &o);
    assert_eq!(polygon.alpha[10 * 20 + 10], 255);
    assert_eq!(polygon.alpha[0], 0);
    let mut rounded = o.clone();
    rounded.corner = 0.5;
    let rounded = geometry(Action::RoundedMarquee, &rounded);
    assert!(rounded.alpha[4 * 20 + 4] < polygon.alpha[4 * 20 + 4]);
    let sector = geometry(Action::SectorMarquee, &o);
    assert_eq!(sector.alpha[12 * 20 + 12], 255);
    assert_eq!(sector.alpha[7 * 20 + 7], 0);
    let mut placed = plane(&[255; 400], 20);
    placed
        .geometry(Action::PerspectiveSelection, &o, None, &CancelToken::new())
        .expect("perspective");
    assert_eq!(placed.alpha[0], 0);
    assert_eq!(placed.alpha[10 * 20 + 10], 255);
    let mut invalid = o.clone();
    invalid.vertices.swap(1, 3);
    assert!(placed
        .geometry(
            Action::PerspectiveSelection,
            &invalid,
            None,
            &CancelToken::new()
        )
        .is_err());
    let mut source = plane(&[0, 0, 0, 0, 255, 0, 0, 0, 0], 3);
    let other = plane(&[0, 0, 0, 0, 0, 255, 0, 0, 0], 3);
    source
        .geometry(Action::AlignCentroid, &o, Some(&other), &CancelToken::new())
        .expect("align");
    assert_eq!(source.alpha, other.alpha);
    let mut fit = plane(&[0, 0, 0, 0, 255, 0, 0, 0, 0], 3);
    let mut full = o;
    full.bounds = [0.0, 0.0, 1.0, 1.0];
    fit.geometry(Action::FitBounds, &full, None, &CancelToken::new())
        .expect("fit");
    assert_eq!(fit.alpha, vec![255; 9]);
}
#[test]
fn topology_tools_select_negative_space_distance_skeleton_otsu_partition_and_shape() {
    let o = options();
    let ring = [255, 255, 255, 255, 0, 255, 255, 255, 255];
    assert_eq!(
        topology(&ring, 3, Action::NegativeSpace, &o),
        vec![0, 0, 0, 0, 255, 0, 0, 0, 0]
    );
    let falloff = topology(&[255; 25], 5, Action::InteriorFalloff, &o);
    assert!(falloff[12] > falloff[0]);
    assert!(falloff[12] < 255);
    let mut block = [0; 49];
    for y in 1..6 {
        for x in 1..6 {
            block[y * 7 + x] = 255;
        }
    }
    let skeleton = topology(&block, 7, Action::Skeleton, &o);
    assert!(skeleton.iter().filter(|a| **a > 0).count() < 25);
    assert_eq!(skeleton[24], 255);
    assert_eq!(
        topology(&[0, 17, 34, 221, 238, 255], 6, Action::AutoThreshold, &o),
        vec![0, 0, 0, 255, 255, 255]
    );
    let mut partition = o.clone();
    partition.vertices = vec![[0.1, 0.5], [0.9, 0.5]];
    let p = topology(&[255; 8], 8, Action::VoronoiPartition, &partition);
    assert_eq!(p, vec![255, 255, 255, 255, 0, 0, 0, 0]);
    let mut filter = o;
    filter.min_area = 2;
    let filtered = topology(
        &[255, 0, 255, 255, 0, 0, 0, 0],
        4,
        Action::FilterRegions,
        &filter,
    );
    assert_eq!(filtered, vec![0, 0, 255, 255, 0, 0, 0, 0]);
    let p = plane(&[255, 0, 255, 255], 4);
    let groups = p.region_groups(&CancelToken::new()).expect("groups");
    assert_eq!(groups.len(), 2);
    assert_eq!(p.isolate(&groups[1]).alpha, vec![0, 0, 255, 255]);
}

#[test]
fn skeleton_thins_frame_edges_and_preserves_single_cell_lines() {
    let result = topology(&[255; 25], 5, Action::Skeleton, &options());
    assert_eq!(result.iter().filter(|a| **a > 0).count(), 1);
    assert_eq!(result[12], 255);
    assert_eq!(
        topology(&[255; 5], 5, Action::Skeleton, &options()),
        vec![255; 5]
    );
    let mut p = plane(&[255; 25], 5);
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(p.topology(Action::Skeleton, &options(), &cancel).is_err());
    assert_eq!(p.alpha, vec![255; 25]);
}

#[test]
fn chroma_key_can_exclude_every_selected_pixel() {
    let frame = Frame::from_rgb(&[0, 255, 0, 0, 255, 0], 2, 1).expect("green frame");
    let mut p = plane(&[255, 128], 2);
    p.pixel_selection(
        Action::ChromaKey,
        &options(),
        [0.5, 0.5],
        &frame,
        &CancelToken::new(),
    )
    .expect("complete key exclusion is a valid empty selection");
    assert_eq!(p.alpha, vec![0; 2]);
}
#[test]
fn sampled_tools_use_source_pixels_and_colour_wand_rejects_disconnected_matches() {
    let rgb = [
        [255, 0, 0],
        [0, 0, 255],
        [255, 0, 0],
        [0, 255, 0],
        [128, 128, 128],
        [255, 255, 255],
    ];
    let bytes: Vec<u8> = rgb.into_iter().flatten().collect();
    let frame = Frame::from_rgb(&bytes, 6, 1).expect("frame");
    let mut o = options();
    o.radius = 1;
    o.tolerance = 0.1;
    let token = CancelToken::new();
    let mut global = plane(&[0; 6], 6);
    global
        .pixel_selection(Action::SampleColour, &o, [0.01, 0.5], &frame, &token)
        .expect("global");
    assert_eq!(global.alpha[0], 255);
    assert_eq!(global.alpha[2], 255);
    assert_eq!(global.alpha[1], 0);
    let mut wand = plane(&[0; 6], 6);
    wand.pixel_selection(Action::ColourWand, &o, [0.01, 0.5], &frame, &token)
        .expect("wand");
    assert_eq!(wand.alpha, vec![255, 0, 0, 0, 0, 0]);
    let mut sat = plane(&[0; 6], 6);
    o.range = [0.9, 1.0];
    sat.pixel_selection(Action::SaturationRange, &o, [0.0, 0.0], &frame, &token)
        .expect("saturation");
    assert_eq!(sat.alpha[0], 255);
    assert_eq!(sat.alpha[4], 0);
    let mut key = plane(&[255; 6], 6);
    key.pixel_selection(Action::ChromaKey, &o, [0.01, 0.5], &frame, &token)
        .expect("key");
    assert_eq!(key.alpha[0], 0);
    assert_eq!(key.alpha[2], 0);
    assert_eq!(key.alpha[1], 255);
    o.tolerance = 1.0;
    let mut lum = plane(&[0; 6], 6);
    lum.pixel_selection(Action::SampleLuminance, &o, [0.5, 0.5], &frame, &token)
        .expect("sample lum");
    assert!(lum.alpha[4] > lum.alpha[1]);
}
#[test]
fn image_guidance_edges_texture_and_constrained_growth_change_coverage() {
    let bytes: Vec<u8> = (0..25)
        .flat_map(|i| {
            if i % 5 < 2 {
                [0, 0, 0]
            } else {
                [255, 255, 255]
            }
        })
        .collect();
    let frame = Frame::from_rgb(&bytes, 5, 5).expect("frame");
    let mut o = options();
    o.radius = 1;
    o.tolerance = 0.1;
    o.range = [0.4, 1.0];
    let token = CancelToken::new();
    let mut edges = plane(&[0; 25], 5);
    edges
        .pixel_selection(Action::ImageEdges, &o, [0.0, 0.0], &frame, &token)
        .expect("edges");
    assert!(edges.alpha[11] > edges.alpha[10]);
    let mut texture = plane(&[0; 25], 5);
    texture
        .pixel_selection(Action::TextureRange, &o, [0.0, 0.0], &frame, &token)
        .expect("texture");
    assert!(texture.alpha[11] > texture.alpha[10]);
    let source = [
        0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0,
        255, 255, 255,
    ];
    let mut guided = plane(&source, 5);
    guided
        .pixel_selection(Action::GuidedSmooth, &o, [0.0, 0.0], &frame, &token)
        .expect("guided");
    assert_eq!(guided.alpha[11], 0);
    assert_eq!(guided.alpha[12], 255);
    let mut seed = plane(
        &[
            255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        5,
    );
    o.radius = 8;
    seed.pixel_selection(Action::EdgeGrow, &o, [0.0, 0.0], &frame, &token)
        .expect("edgegrow");
    assert_eq!(seed.alpha[20], 255);
    assert_eq!(seed.alpha[12], 0);
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(seed
        .pixel_selection(Action::EdgeGrow, &o, [0.0, 0.0], &frame, &cancelled)
        .is_err());
}
#[test]
fn laboratory_payload_bounds_and_invalid_input_cannot_reach_saving() {
    let mut input = request(Action::Freeze);
    input.advanced = Some(options());
    assert!(Options::validated(&input).is_ok());
    input.advanced.as_mut().expect("options").vertices = vec![[0.2, 0.2]; 129];
    assert!(Options::validated(&input).is_err());
    input.advanced = Some(options());
    input.advanced.as_mut().expect("options").tolerance = f32::NAN;
    assert!(Options::validated(&input).is_err());
    input.advanced = Some(options());
    input.point = Some([2.0, 0.2]);
    assert!(Options::validated(&input).is_err());
}
const ACTIONS: [Action; 25] = [
    Action::PolygonMarquee,
    Action::RoundedMarquee,
    Action::SectorMarquee,
    Action::SampleColour,
    Action::ColourWand,
    Action::SampleLuminance,
    Action::SaturationRange,
    Action::TextureRange,
    Action::ImageEdges,
    Action::GuidedSmooth,
    Action::EdgeGrow,
    Action::InteriorFalloff,
    Action::NegativeSpace,
    Action::Skeleton,
    Action::SplitRegions,
    Action::AppendStack,
    Action::ReplaceSelection,
    Action::SyncAdjustments,
    Action::AlignCentroid,
    Action::FitBounds,
    Action::PerspectiveSelection,
    Action::AutoThreshold,
    Action::ChromaKey,
    Action::VoronoiPartition,
    Action::FilterRegions,
];
pub(super) fn assert_persisted(state: &AppState, project: ProjectId, photo: PhotoId) {
    let mut baseline =
        crate::develop_commands::load_or_neutral(state, photo).expect("load baseline");
    let mut one = authored_mask();
    one.components = vec![
        Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Radial {
                centre: [0.2, 0.5],
                radii: [0.12, 0.2],
                angle: 0.0,
                feather: 0.2,
            },
        },
        Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Radial {
                centre: [0.75, 0.5],
                radii: [0.12, 0.2],
                angle: 0.0,
                feather: 0.2,
            },
        },
    ];
    let mut two = one.clone();
    two.id = "two".into();
    two.components.truncate(1);
    two.params.exposure = Some(-0.3);
    two.amount = 0.8;
    local_masks::write(&mut baseline, &[one.clone(), two.clone()], &BTreeMap::new())
        .expect("baseline masks");
    let mut color = aura_recipe::studio_color::read(&baseline).expect("color");
    color.local.insert(
        "one".into(),
        aura_recipe::studio_color::LocalColor {
            vibrance: 12.0,
            ..Default::default()
        },
    );
    color.local.insert(
        "two".into(),
        aura_recipe::studio_color::LocalColor {
            vibrance: -12.0,
            ..Default::default()
        },
    );
    aura_recipe::studio_color::write(&mut baseline, &color).expect("baseline color");
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &baseline,
            &[
                local_masks::KEY.into(),
                aura_recipe::studio_color::KEY.into(),
            ],
            "Laboratory baseline",
        )
        .expect("baseline save");
    let baseline = crate::develop_commands::load_or_neutral(state, photo).expect("stored baseline");
    let hash = aura_recipe::recipe_hash(&baseline).expect("baseline hash");
    let render = || {
        crate::develop_commands::render_image(
            state,
            &crate::contract::ipc::RenderImageInput {
                photo_id: photo.to_db(),
                level: Some("full".into()),
                screen: None,
                colour_space: None,
                purpose: Some("export".into()),
            },
        )
        .expect("native export render")
        .rgb_base64
    };
    let baseline_render = render();
    let mut bounded_split = request(Action::Freeze);
    bounded_split.action = Action::SplitRegions;
    bounded_split.project_id = project.to_db();
    bounded_split.photo_id = photo.to_db();
    bounded_split.recipe_hash = hash.clone();
    bounded_split.cancel_id = uuid::Uuid::new_v4().to_string();
    bounded_split.advanced = Some(Options {
        min_area: 1,
        max_area: 1,
        ..options()
    });
    assert!(
        apply(state, &bounded_split).is_err(),
        "split respects maximum region area"
    );
    assert_eq!(
        aura_recipe::recipe_hash(
            &crate::develop_commands::load_or_neutral(state, photo).expect("unchanged baseline")
        )
        .expect("hash"),
        hash
    );
    for action in ACTIONS {
        let mut input = request(Action::Freeze);
        input.action = action;
        input.project_id = project.to_db();
        input.photo_id = photo.to_db();
        input.recipe_hash = hash.clone();
        input.cancel_id = uuid::Uuid::new_v4().to_string();
        input.point = Some([0.2, 0.5]);
        let mut o = options();
        o.min_area = 1;
        o.range = [0.0, 1.0];
        input.advanced = Some(o);
        if matches!(
            action,
            Action::AppendStack
                | Action::ReplaceSelection
                | Action::SyncAdjustments
                | Action::AlignCentroid
        ) {
            input.other_mask_id = Some("two".into());
        }
        let edited = apply(state, &input).unwrap_or_else(|e| panic!("{action:?}: {e:?}"));
        assert_ne!(
            edited.recipe.recipe_hash, hash,
            "{action:?} saves an actual change"
        );
        let saved = crate::develop_commands::load_or_neutral(state, photo).expect("saved");
        let saved_render = render();
        if action != Action::FilterRegions {
            assert_ne!(
                saved_render, baseline_render,
                "{action:?} changes actual native export pixels"
            );
        }
        assert_eq!(saved.global, baseline.global);
        let masks = local_masks::read(&saved).expect("saved masks");
        if action == Action::SyncAdjustments {
            assert_eq!(masks[1].params, one.params);
            assert_eq!(masks[1].amount, one.amount);
            let color = aura_recipe::studio_color::read(&saved).expect("synced color");
            assert_eq!(color.local.get("one"), color.local.get("two"));
        } else {
            assert_eq!(masks[0].params, one.params);
            assert_eq!(masks[1].params, two.params);
        }
        if action == Action::SplitRegions {
            assert_eq!(masks.len(), 4);
            let c = aura_recipe::studio_color::read(&saved).expect("region colors");
            for region in &masks[2..] {
                assert_eq!(c.local.get(&region.id), c.local.get("one"));
            }
        }
        assert!(
            apply(state, &input).is_err(),
            "{action:?} stale request refused"
        );
        let history = |action: &str| {
            crate::develop_commands::history_step(
                state,
                &crate::contract::ipc::HistoryStepInput {
                    project_id: project.to_db(),
                    photo_id: photo.to_db(),
                    action: action.into(),
                },
            )
            .expect("history step")
        };
        history("undo");
        assert_eq!(
            aura_recipe::recipe_hash(
                &crate::develop_commands::load_or_neutral(state, photo).expect("undo")
            )
            .expect("hash"),
            hash,
            "{action:?} single-step exact undo"
        );
        history("redo");
        assert_eq!(
            aura_recipe::recipe_hash(
                &crate::develop_commands::load_or_neutral(state, photo).expect("redo")
            )
            .expect("hash"),
            edited.recipe.recipe_hash,
            "{action:?} exact redo"
        );
        assert_eq!(
            render(),
            saved_render,
            "{action:?} redo restores native export pixels"
        );
        history("undo");
    }
}
