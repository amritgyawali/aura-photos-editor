//! The ninth grep-as-a-test: a phase that cannot be right must own no output.
//!
//! After `aura-render/tests/colour_discipline.rs`, `aura-brain-photo/tests/no_recipe_writes.rs`,
//! `aura-vision/tests/no_template_writes.rs`, `aura-geometry/tests/no_render_calls.rs`,
//! `aura-generative/tests/one_choke_point.rs`, `aura-brain-gallery/tests/no_recipe_writes.rs`,
//! `aura-qc/tests/no_pixel_ops.rs` and `aura-jobs/tests/no_decisions.rs`.
//!
//! ## What this is guarding
//!
//! Curation is the first thing in this product that is a matter of taste, and a phase that cannot be
//! *right* has exactly one way to be safe: own no output. ADR-0059 section 3.
//!
//! The thing that makes this crate the one that most needs the grep is how easy the unsafe version
//! would be. This phase solves a monochrome conversion. The `bw` block phase 14 froze is two
//! fields, `aura_recipe::schema::merge` is one call away, and a photographer would open the gallery
//! and see something beautiful. That is the product deciding a wedding is monochrome, and section
//! 6.1 says not to.
//!
//! The manifest is the first line of defence - `aura-curate` depends on `aura-core`,
//! `aura-catalog`, `aura-index` and `aura-cloud`, and on none of the deciding crates - and this is
//! the second, which catches the version where somebody adds the dependency and the call in one
//! commit.
//!
//! ## Why it strips comments and string literals first
//!
//! Phase 27 learned this twice in one phase: a check that reads documentation as if it were code
//! fails hardest on the codebases that document themselves best. This crate's doc comments say
//! "recipe", "render" and "apply" on nearly every page, because explaining what a curation engine
//! must not do requires naming it - and `curation.toml`'s own header is four paragraphs about why
//! there is no skin target in it.

use std::path::Path;

/// Every `.rs` file in the crate's source, with comments and string literals removed.
///
/// String literals go too, which is one more than the earlier greps strip. This crate's error
/// messages and reason texts are full of the vocabulary being banned - "waiting for you, because
/// AURA is not confident enough" - and a scan that read them would fail on the sentences that
/// exist to explain the rule.
fn sources() -> Vec<(String, String)> {
    raw_sources()
        .into_iter()
        .map(|(name, text)| (name, strip(&strip_tests(&text))))
        .collect()
}

/// Every `.rs` file in the crate's source, exactly as written.
///
/// What [`only_set_order_touches_the_photographers_own_album_order`] reads, because the thing it is
/// looking for lives in a **string literal** - SQL - and [`strip`] removes those on purpose.
fn raw_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    collect(&root, &mut out);
    out
}

/// Remove `#[cfg(test)] mod tests { ... }`.
///
/// The third time this repository has met the same defect, and the first time it has been an
/// *identifier* rather than a comment. Phase 27 found a grep matching its own test's name and a
/// schema scan matching a migration's own prose; this one matched
/// `fn a_key_naming_a_skin_target_is_refused`, which is the test that proves the rule holds.
///
/// A test module is not compiled into the library at all, so nothing in it can reach a
/// photographer - which is the same argument the crate's own lint exemptions make.
fn strip_tests(text: &str) -> String {
    let Some(start) = text.find("#[cfg(test)]") else {
        return text.to_string();
    };
    let Some(brace) = text[start..].find('{') else {
        return text[..start].to_string();
    };
    let mut depth = 0i32;
    let bytes: Vec<char> = text[start + brace..].chars().collect();
    for (offset, ch) in bytes.iter().enumerate() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + brace + offset + 1;
                    let mut out = text[..start].to_string();
                    out.push_str(&strip_tests(&text[end..]));
                    return out;
                }
            }
            _ => {}
        }
    }
    text[..start].to_string()
}

fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), text));
            }
        }
    }
}

/// Remove line comments, block comments and double-quoted string literals.
///
/// A small state machine rather than a regular expression, because the thing being removed nests
/// and a regex that got the nesting wrong would silently delete code - which is a much worse
/// failure than the one this file exists to prevent. Phase 27's `strip_sql_comments` made the same
/// argument about block comments in SQL.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    let mut depth = 0usize;
    while index < bytes.len() {
        let current = bytes.get(index).copied().unwrap_or(' ');
        let next = bytes.get(index + 1).copied();

        if depth > 0 {
            if current == '*' && next == Some('/') {
                depth -= 1;
                index += 2;
                continue;
            }
            if current == '/' && next == Some('*') {
                depth += 1;
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }

        if current == '/' && next == Some('*') {
            depth = 1;
            index += 2;
            continue;
        }
        if current == '/' && next == Some('/') {
            while index < bytes.len() && bytes.get(index).copied() != Some('\n') {
                index += 1;
            }
            continue;
        }
        if current == '"' {
            index += 1;
            while index < bytes.len() {
                let c = bytes.get(index).copied().unwrap_or('"');
                if c == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
                if c == '"' {
                    break;
                }
            }
            out.push(' ');
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

fn assert_absent(needle: &str, why: &str) {
    for (name, text) in sources() {
        assert!(!text.contains(needle), "{name} contains `{needle}`. {why}");
    }
}

#[test]
fn curation_never_writes_a_recipe() {
    // Phase 14's rule, fourteenth application, and the one this crate would break most easily.
    assert_absent(
        "schema::merge",
        "A monochrome mix is a proposal. Writing it into a recipe is the develop surface's job, \
         with a photographer behind it.",
    );
    assert_absent("aura_recipe", "This crate has no recipe dependency.");
    assert_absent("aura_render", "This crate has no renderer.");
}

#[test]
fn curation_opens_no_file_and_reaches_no_socket() {
    // `CurateService::export` returns a `String`; the shell saves it. Two export paths is two
    // answers to what was delivered, and phase 30 owns the other one.
    for needle in [
        "std::fs",
        "File::create",
        "File::open",
        "OpenOptions",
        "write_all",
        "TcpStream",
        "reqwest",
        "ureq",
    ] {
        assert_absent(
            needle,
            "Nothing in phase 29 writes a file or opens a socket; the export is text.",
        );
    }
}

#[test]
fn curation_opens_no_photograph() {
    // Every reading is a number an earlier phase measured and stored. That is phase 05's rule -
    // descriptors are computed once - and it is what makes a whole gallery affordable inside
    // section 11's 20 s budget.
    for needle in ["aura_preview", "aura_raw", "PreviewService", "decode("] {
        assert_absent(
            needle,
            "Nothing in phase 29 opens a photograph; it reads stored descriptors.",
        );
    }
}

#[test]
fn curation_keeps_no_similarity_index_of_its_own() {
    // Phase 05's rule: `SimilarityIndex` is the only way to ask what looks like something. Every
    // distance in this crate arrives through the `Field` port.
    for needle in ["fn cosine", "fn hamming", "dot_product", "hnsw"] {
        assert_absent(
            needle,
            "Similarity comes through phase 05's frozen index, never from a second implementation.",
        );
    }
}

#[test]
fn curation_keeps_no_coverage_engine_of_its_own() {
    // Phase 12 owns which frames satisfy which guarantee. This crate does subset arithmetic over
    // the answer, and a second rule table would be a second answer to "is the ring exchange in the
    // gallery".
    for needle in ["aura_cull", "RuleTable", "coverage_rules"] {
        assert_absent(
            needle,
            "The coverage vocabulary and the rule table are phase 12's.",
        );
    }
}

#[test]
fn no_type_in_this_crate_carries_a_skin_target() {
    // The band a monochrome mix protects is looked up per identity from phase 15's measured loci.
    // A constant here would be the thing `docs/skin-fairness.md` says this product does not have,
    // and a monochrome conversion is where it would be least visible.
    for needle in [
        "SKIN_TARGET",
        "IDEAL_SKIN",
        "skin_target",
        "ideal_skin",
        "target_skin",
        "REFERENCE_SKIN",
    ] {
        assert_absent(
            needle,
            "The skin band is measured per person from `ToneService::skin_loci`.",
        );
    }
}

#[test]
fn nothing_here_can_change_which_photographs_are_delivered() {
    // Phase 12 owns the gallery. A curation engine that could remove a frame would be a curation
    // engine that decides what a wedding's record is.
    for needle in [
        "fn delete",
        "fn remove_from_gallery",
        "DELETE FROM photo",
        "set_mode(",
    ] {
        assert_absent(
            needle,
            "Curation proposes. The gallery is phase 12's and delivery is phase 30's.",
        );
    }
}

#[test]
fn only_set_order_touches_the_photographers_own_album_order() {
    // The first of the three statements that keep a photographer's reorder alive across a re-run.
    // The others are `album_order.source` and the `curate_album_no_reorder` trigger.
    //
    // Read over the **raw** sources rather than the stripped ones, because the thing being counted
    // is a table name inside a SQL string literal and `strip` removes those. A scan that used the
    // stripped text here would find nothing and pass on every build, which is the worst kind of
    // green.
    let mut writers = Vec::new();
    for (name, text) in raw_sources() {
        if !strip_tests(&text).contains("album_order") {
            continue;
        }
        writers.push(name);
    }
    assert_eq!(
        writers.len(),
        1,
        "exactly one module may name `album_order`, and it is `store.rs`: {writers:?}"
    );
    assert!(
        writers
            .first()
            .is_some_and(|name| name.ends_with("store.rs")),
        "{writers:?}"
    );
}

#[test]
fn the_grep_reads_code_rather_than_the_prose_explaining_it() {
    // Phase 27's lesson, and this crate is the one where it bites hardest: the module headers say
    // "recipe", "render" and "apply" on nearly every page because explaining the rule requires
    // naming it.
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/policy.rs"),
    )
    .expect("policy.rs");
    assert!(
        raw.contains("skin_target"),
        "the loader's own refusal list names the thing it refuses"
    );
    assert!(
        !strip(&strip_tests(&raw)).contains("skin_target"),
        "and the scan must not read the refusal list, or the code enforcing the rule fails it"
    );

    // The same, for a comment rather than a literal.
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("lib.rs");
    assert!(
        lib.contains("recipe writer"),
        "the header should explain the rule"
    );
}
