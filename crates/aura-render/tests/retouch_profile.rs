//! Where the time goes when a retouch stack renders at full resolution. Run by hand:
//! `AURA_PROFILE_RGB=photo.rgb AURA_PROFILE_SIZE=2048x3072 AURA_PROFILE_RECIPE=recipe.json`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

#[test]
#[ignore = "requires AURA_PROFILE_* pointing at a real photograph and recipe"]
fn time_every_retouch_operation() {
    let rgb = std::fs::read(std::env::var("AURA_PROFILE_RGB").unwrap()).unwrap();
    let size = std::env::var("AURA_PROFILE_SIZE").unwrap();
    let (w, h) = size.split_once('x').unwrap();
    let (w, h): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
    let recipe: aura_recipe::Recipe = serde_json::from_str(
        &std::fs::read_to_string(std::env::var("AURA_PROFILE_RECIPE").unwrap()).unwrap(),
    )
    .unwrap();
    let edits = aura_recipe::retouch_tools::read(&recipe).unwrap();
    let mattes = aura_recipe::retouch_tools::read_mattes(&recipe).unwrap();
    let mut pixels: Vec<f32> = rgb
        .iter()
        .map(|v| aura_raw::colour::curve::srgb_decode(f32::from(*v) / 255.0))
        .collect();
    let mut rows = Vec::new();
    let total = std::time::Instant::now();
    let whole = std::time::Instant::now();
    aura_render::retouch_tools::apply_with_mattes(&mut pixels.clone(), w, h, &edits, &mattes);
    println!(
        "whole stack in one pass: {} ms",
        whole.elapsed().as_millis()
    );
    for edit in &edits {
        let started = std::time::Instant::now();
        aura_render::retouch_tools::apply_with_mattes(
            &mut pixels,
            w,
            h,
            std::slice::from_ref(edit),
            &mattes,
        );
        rows.push((
            started.elapsed().as_millis(),
            edit.id.clone(),
            format!("{:?}", edit.tool),
        ));
    }
    println!(
        "{} operations, {} ms in all",
        edits.len(),
        total.elapsed().as_millis()
    );
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    for (ms, id, tool) in rows.iter().take(25) {
        println!("{ms:>7} ms  {tool:<18} {id}");
    }
}
