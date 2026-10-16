//! Body-only segmentation for crops that contain no detectable face.
use super::{
    colour_model, components, guided, invalid, segment, to_matte, working, Class, ColourModel,
    Field, Image, Matte, Options, SIDE, WORKING_EDGE,
};
use aura_core::AuraResult;

/// Above this, a competing label is the network's answer and colour cannot overrule it.
const SURE_OF_SOMETHING_ELSE: f32 = 0.75;
/// How like the body's own measured skin a cell must be to join it.
const OWN_SKIN: f32 = 0.5;
/// The selection the network made is what the colour model is measured on and what completion
/// grows from; it may at most double. Skin the network missed is a hole or a margin, and a
/// selection that wants to grow further than that has found a wall, not skin.
const MOST_GROWTH: f32 = 1.0;
/// The coverage a joined cell is given: the network's own floor for calling a cell sure.
const JOINED: f32 = 0.9;
/// The share of an enclosed island that must be the body's own skin colour for it to join.
const MOSTLY_SKIN: f32 = 0.8;

/// The cells of every enclosed island that is this body's own skin. See [`complete`].
fn islands(image: &Image, field: &Field, support: &[f32], model: &ColourModel) -> Vec<usize> {
    let (w, h) = (image.w, image.h);
    // Everything unselected that the frame's border can reach is outside the body.
    let mut outside = vec![false; support.len()];
    let mut frontier: Vec<usize> = (0..support.len())
        .filter(|&i| {
            let (x, y) = (i % w, i / w);
            support[i] == 0.0 && (x == 0 || y == 0 || x + 1 == w || y + 1 == h)
        })
        .collect();
    for &i in &frontier {
        outside[i] = true;
    }
    while let Some(i) = frontier.pop() {
        let (x, y) = (i % w, i / w);
        let around = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        for n in around.into_iter().flatten() {
            if support[n] == 0.0 && !outside[n] {
                outside[n] = true;
                frontier.push(n);
            }
        }
    }
    let enclosed: Vec<bool> = (0..support.len())
        .map(|i| support[i] == 0.0 && !outside[i])
        .collect();
    let (labels, count) = components(&enclosed, w, h);
    let mut cells = vec![0_usize; count + 1];
    let mut own = vec![0_usize; count + 1];
    for (i, &label) in labels.iter().enumerate() {
        let label = label as usize;
        if label == 0 {
            continue;
        }
        cells[label] += 1;
        // Something worn or grown is never skin, whatever colour it is.
        let worn = [Class::Clothes, Class::Hair, Class::FaceSkin]
            .iter()
            .any(|c| field.plane(*c)[i] > SURE_OF_SOMETHING_ELSE);
        let like = model.likeness(image.pixel(i % w, i / w), 0.7, 0.5);
        own[label] += usize::from(!worn && like >= OWN_SKIN);
    }
    labels
        .iter()
        .enumerate()
        .filter(|(_, &label)| {
            let label = label as usize;
            label != 0 && own[label] as f32 >= cells[label] as f32 * MOSTLY_SKIN
        })
        .map(|(i, _)| i)
        .collect()
}

fn body_probability(field: &Field) -> Vec<f32> {
    field
        .plane(Class::BodySkin)
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let other = Class::ALL
                .iter()
                .filter(|c| **c != Class::BodySkin)
                .map(|c| field.plane(*c)[i])
                .fold(0.0_f32, f32::max);
            // A missing face detector cannot authorize healing facial features. Use
            // body skin only, with the network's competing labels as an explicit veto.
            if *body > other && *body > 0.5 {
                *body
            } else {
                0.0
            }
        })
        .collect()
}

/// Skin the network left out of a body it did find.
///
/// The segmenter learned people as they usually look. Skin that carries dense marks - a
/// stretch-marked abdomen, a scar, a rash - is often given to no label at all, and a selection
/// with a hole in the middle of a belly heals up to a line across the belly. A cell joins when
/// it touches skin already selected, is this body's own skin colour as measured on the cells
/// the network was sure of, and no competing label is sure of it. Nothing is compared with a
/// skin colour anybody chose: the model is the selected skin's own median and spread.
///
/// An island is different. Unselected cells that cannot be reached from the frame's border
/// without crossing selected skin are surrounded by this body, and there the network being
/// sure of "something else" is not evidence of a thing: a heavily marked abdomen under two
/// hands is labelled an object with nine-tenths confidence. An island joins whole when most
/// of it is the body's own colour and not claimed as clothing or hair, so a watch, a
/// bracelet, a tattoo or a swimsuit stays out.
///
/// All or nothing: growth that would more than double the selection is discarded whole.
fn complete(image: &Image, field: &Field, support: &mut [f32]) {
    let (w, h) = (image.w, image.h);
    let confident: Vec<usize> = (0..support.len()).filter(|&i| support[i] > 0.8).collect();
    let Some(model) = colour_model(image, &confident) else {
        return;
    };
    let selected = support.iter().filter(|v| **v > 0.0).count();
    let budget = (selected as f32 * MOST_GROWTH) as usize;
    let likeness = |i: usize| -> f32 {
        let contested = Class::ALL
            .iter()
            .filter(|c| **c != Class::BodySkin)
            .any(|c| field.plane(*c)[i] > SURE_OF_SOMETHING_ELSE);
        if contested {
            0.0
        } else {
            model.likeness(image.pixel(i % w, i / w), 0.7, 0.5)
        }
    };
    let mut joined: Vec<usize> = Vec::new();
    let mut seen: Vec<bool> = support.iter().map(|v| *v > 0.0).collect();
    let mut frontier: Vec<usize> = (0..support.len()).filter(|&i| support[i] > 0.5).collect();
    while let Some(i) = frontier.pop() {
        let (x, y) = (i % w, i / w);
        let around = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        for n in around.into_iter().flatten() {
            if seen[n] {
                continue;
            }
            seen[n] = true;
            if likeness(n) >= OWN_SKIN {
                joined.push(n);
                frontier.push(n);
            }
        }
        if joined.len() > budget {
            return;
        }
    }
    // Joined cells are selected as firmly as the skin they joined. Carrying each cell's own
    // likeness into the selection instead would print the marks it was grown across into the
    // selection itself, and healing then follows that pattern rather than the skin.
    let mut grown = support.to_vec();
    for &i in &joined {
        grown[i] = JOINED;
    }
    // Islands are looked for in the grown selection: it is often the margin joined above
    // that closes one.
    let inside = islands(image, field, &grown, &model);
    if joined.len() + inside.len() > budget {
        return;
    }
    for i in inside {
        grown[i] = JOINED;
    }
    support.copy_from_slice(&grown);
}

/// Select visible body skin without requiring a face or inventing face landmarks.
/// # Errors
/// Invalid pixels, unavailable bundled segmentation, or inference failure.
pub fn analyse_body(
    rgb: &[u8],
    width: u32,
    height: u32,
    options: Options,
) -> AuraResult<(Option<Matte>, usize)> {
    let (w, h) = (width as usize, height as usize);
    if w < 8 || h < 8 || w.checked_mul(h).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
        return Err(invalid("Invalid body skin analysis pixels"));
    }
    let image = working(rgb, w, h, WORKING_EDGE);
    let (field, passes) = segment(&image, &[], 0)?;
    let mut support = body_probability(&field);
    complete(&image, &field, &mut support);
    let softness = if options.softness.is_finite() {
        options.softness.clamp(0.0, 1.0)
    } else {
        0.35
    };
    let cell = (image.w.max(image.h) as f32 / SIDE as f32).max(1.0);
    let radius = ((cell * (0.8 + softness * 2.2)).round() as usize).max(1);
    let mut refined = guided(
        &image.luma(),
        &support,
        image.w,
        image.h,
        radius,
        0.0004 + softness * 0.004,
    );
    for (value, allowed) in refined.iter_mut().zip(&support) {
        if *allowed == 0.0 {
            *value = 0.0;
        }
    }
    Ok((to_matte(&refined, image.w, image.h, 0.03), passes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn body_only_detection_rejects_face_clothing_hair_and_background() {
        let mut field = Field {
            planes: vec![vec![0.02; 6]; 6],
        };
        for i in 0..6 {
            field.planes[i][i] = 0.9;
        }
        let body = body_probability(&field);
        for (i, probability) in body.iter().enumerate() {
            let expected = if i == Class::BodySkin as usize {
                0.9
            } else {
                0.0
            };
            assert!((*probability - expected).abs() < f32::EPSILON);
        }
    }

    /// A frame labelled cell by cell: the named class at `sure`, the rest shared evenly.
    fn labelled(w: usize, h: usize, label: impl Fn(usize, usize) -> (Class, f32)) -> Field {
        let mut planes = vec![vec![0.0; w * h]; 6];
        for i in 0..w * h {
            let (class, sure) = label(i % w, i / w);
            for (c, plane) in planes.iter_mut().enumerate() {
                plane[i] = if c == class as usize {
                    sure
                } else {
                    (1.0 - sure) / 5.0
                };
            }
        }
        Field { planes }
    }

    /// Skin with a little of its own variation, so the colour model has a spread to measure.
    fn skin(x: usize, y: usize) -> [u8; 3] {
        [
            200 + ((x * 7 + y * 3) % 9) as u8,
            150 + ((x + y * 5) % 7) as u8,
            130 + ((x * 3 + y) % 5) as u8,
        ]
    }

    fn painted(w: usize, h: usize, colour: impl Fn(usize, usize) -> [u8; 3]) -> Image {
        Image {
            w,
            h,
            rgb: (0..w * h).flat_map(|i| colour(i % w, i / w)).collect(),
        }
    }

    #[test]
    fn skin_the_network_was_unsure_of_joins_the_body_it_sits_in() {
        let (w, h) = (48, 32);
        let hole = |x: usize, y: usize| (10..20).contains(&x) && (10..22).contains(&y);
        // Skin on the left, grey clothing, then a wall of exactly the same skin colour.
        let image = painted(w, h, |x, y| {
            if (30..36).contains(&x) {
                [90, 90, 95]
            } else {
                skin(x, y)
            }
        });
        let field = labelled(w, h, |x, y| {
            if x >= 36 {
                (Class::Background, 0.92)
            } else if x >= 30 {
                (Class::Clothes, 0.9)
            } else if hole(x, y) {
                // What a stretch-marked abdomen measured: no label is sure of it.
                (Class::Other, 0.52)
            } else {
                (Class::BodySkin, 0.92)
            }
        });
        let mut support = body_probability(&field);
        assert!(support[15 * w + 15].abs() < f32::EPSILON);
        complete(&image, &field, &mut support);
        for y in 0..h {
            for x in 0..w {
                let selected = support[y * w + x] > 0.0;
                assert_eq!(selected, x < 30, "cell {x},{y}");
            }
        }
    }

    /// A body filling the frame's left, with an island at `island` painted and labelled.
    fn with_island(colour: [u8; 3], class: Class) -> (Vec<f32>, usize) {
        let (w, h) = (48, 32);
        let island = |x: usize, y: usize| (10..20).contains(&x) && (10..22).contains(&y);
        let image = painted(w, h, |x, y| if island(x, y) { colour } else { skin(x, y) });
        let field = labelled(w, h, |x, y| {
            if island(x, y) {
                (class, 0.9)
            } else if x < 30 {
                (Class::BodySkin, 0.92)
            } else {
                (Class::Background, 0.92)
            }
        });
        let mut support = body_probability(&field);
        complete(&image, &field, &mut support);
        (support, 15 * w + 15)
    }

    #[test]
    fn an_island_of_the_bodys_own_skin_joins_even_when_the_network_calls_it_an_object() {
        // What the lower half of a stretch-marked abdomen under two hands measured.
        let (support, inside) = with_island(skin(3, 4), Class::Other);
        assert!((support[inside] - JOINED).abs() < f32::EPSILON);
    }

    #[test]
    fn an_island_of_another_colour_or_of_clothing_stays_out() {
        // A watch: enclosed by skin, and not skin.
        let (support, inside) = with_island([40, 42, 48], Class::Other);
        assert!(support[inside].abs() < f32::EPSILON);
        // A skin-coloured garment the network is sure is a garment.
        let (support, inside) = with_island(skin(3, 4), Class::Clothes);
        assert!(support[inside].abs() < f32::EPSILON);
    }

    #[test]
    fn a_skin_coloured_wall_touching_the_body_does_not_join_it() {
        let (w, h) = (48, 32);
        let image = painted(w, h, skin);
        let field = labelled(w, h, |x, _| {
            if x < 24 {
                (Class::BodySkin, 0.92)
            } else {
                (Class::Background, 0.92)
            }
        });
        let mut support = body_probability(&field);
        complete(&image, &field, &mut support);
        assert!(support
            .iter()
            .enumerate()
            .all(|(i, v)| (*v > 0.0) == (i % w < 24)));
    }

    #[test]
    fn growth_that_would_more_than_double_the_selection_is_discarded_whole() {
        let (w, h) = (48, 32);
        let image = painted(w, h, skin);
        // A small confident patch in a frame the network is otherwise unsure of throughout.
        let field = labelled(w, h, |x, y| {
            if x < 12 && y < 12 {
                (Class::BodySkin, 0.92)
            } else {
                (Class::Other, 0.52)
            }
        });
        let mut support = body_probability(&field);
        let before = support.clone();
        complete(&image, &field, &mut support);
        assert_eq!(support, before);
    }
}
