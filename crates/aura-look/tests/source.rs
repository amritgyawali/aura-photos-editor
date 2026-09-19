//! What a photographer pasted into the box, and what came back.

use std::fs;

use aura_core::contract::look::{MediaSource, ReferenceOrigin};
use aura_look::source;

#[test]
fn every_way_of_writing_an_instagram_page_parses_to_one_handle() {
    for text in [
        "https://www.instagram.com/some.photographer/",
        "http://instagram.com/some.photographer",
        "instagram.com/some.photographer/?hl=en",
        "www.instagram.com/some.photographer",
        "@some.photographer",
        "some.photographer",
        "  Some.Photographer  ",
    ] {
        let parsed = ReferenceOrigin::parse(text).unwrap_or_else(|error| panic!("{text} did not parse: {error:?}"));
        assert_eq!(
            parsed,
            ReferenceOrigin::Instagram {
                handle: "some.photographer".to_string()
            },
            "{text} parsed to the wrong handle"
        );
    }
}

#[test]
fn a_post_or_a_reel_names_the_page_it_is_on() {
    // This phase is about a page rather than about one photograph on it, so everything after
    // the first path segment is dropped rather than refused.
    let parsed = ReferenceOrigin::parse("https://instagram.com/somebody/p/Cabc123/").expect("the fixture must hold");
    assert_eq!(
        parsed,
        ReferenceOrigin::Instagram {
            handle: "somebody".to_string()
        }
    );
}

#[test]
fn something_that_is_not_instagram_stays_a_web_reference() {
    let parsed = ReferenceOrigin::parse("https://somebody.photography/portfolio").expect("the fixture must hold");
    assert!(matches!(parsed, ReferenceOrigin::Web { .. }));
}

#[test]
fn a_handle_with_a_path_separator_is_refused() {
    // The guard that stops a pasted file path being stored as an account name.
    assert!(ReferenceOrigin::parse("../../etc/passwd").is_err());
    assert!(ReferenceOrigin::parse("").is_err());
    assert!(ReferenceOrigin::parse("   ").is_err());
}

#[test]
fn a_handle_longer_than_instagram_issues_is_refused() {
    let long = "a".repeat(ReferenceOrigin::MAX_HANDLE + 1);
    assert!(ReferenceOrigin::parse(&long).is_err());
}

#[test]
fn an_origin_survives_a_trip_through_the_catalog_key() {
    for origin in [
        ReferenceOrigin::Instagram {
            handle: "somebody".to_string(),
        },
        ReferenceOrigin::Web {
            url: "https://example.test/gallery".to_string(),
        },
        ReferenceOrigin::Local {
            label: "Client mood board".to_string(),
        },
    ] {
        assert_eq!(ReferenceOrigin::from_key(&origin.as_key()), origin);
    }
}

// ---------------------------------------------------------------------------
// The refusal
// ---------------------------------------------------------------------------

#[test]
fn fetching_from_a_page_is_refused_and_says_why() {
    let error = source::resolve(
        "https://instagram.com/somebody",
        MediaSource::PublicUrl,
        None,
    )
    .expect_err("this build must not claim it can fetch a page");

    let message = format!("{error:?}");
    assert!(
        message.contains("TLS") || message.contains("network"),
        "the refusal must say what is missing, got {message}"
    );
    assert!(
        message.contains("folder"),
        "the refusal must name the route that does work, got {message}"
    );
}

#[test]
fn the_contract_and_the_walk_agree_about_which_sources_work() {
    // One place decides, and it is a const on the contract. A panel that offered a route the
    // walk refuses would be a button that always fails.
    assert!(MediaSource::Folder.can_fetch());
    assert!(MediaSource::InstagramExport.can_fetch());
    assert!(!MediaSource::PublicUrl.can_fetch());
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

#[test]
fn the_walk_keeps_one_vote_per_photograph_however_many_copies_there_are() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    let root = dir.path();

    // Three distinct files and two duplicates of the first.
    for (name, body) in [
        ("a.jpg", b"one".as_slice()),
        ("b.jpg", b"two".as_slice()),
        ("c.jpeg", b"three".as_slice()),
        ("copy-of-a.jpg", b"one".as_slice()),
        ("another-copy.jpg", b"one".as_slice()),
    ] {
        fs::write(root.join(name), body).expect("the fixture must hold");
    }
    // And something that is not a photograph.
    fs::write(root.join("notes.txt"), b"not a photograph").expect("the fixture must hold");

    let (files, reasons) = source::walk(root);

    assert_eq!(files.len(), 3, "duplicates were counted more than once");
    assert!(reasons
        .iter()
        .any(|reason| reason.code == aura_core::contract::look::LookCode::ReferenceDuplicate));
    assert!(reasons
        .iter()
        .any(|reason| reason.code == aura_core::contract::look::LookCode::ReferenceNotAnImage));
}

#[test]
fn the_walk_returns_the_same_order_twice() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    let root = dir.path();
    for index in 0..12 {
        fs::write(root.join(format!("{index:02}.jpg")), format!("body-{index}")).expect("the fixture must hold");
    }

    let (one, _) = source::walk(root);
    let (two, _) = source::walk(root);

    assert_eq!(one, two, "two walks of one folder disagreed");
}

#[test]
fn a_folder_with_too_few_photographs_is_refused_rather_than_measured() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    for index in 0..3 {
        fs::write(dir.path().join(format!("{index}.jpg")), format!("{index}")).expect("the fixture must hold");
    }

    let error = source::resolve("", MediaSource::Folder, Some(dir.path()))
        .expect_err("three photographs is not a look");
    let message = format!("{error:?}");
    assert!(
        message.contains("minimum"),
        "the refusal must say what the minimum is, got {message}"
    );
}

#[test]
fn an_instagram_export_layout_is_found_and_named() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    let posts = dir.path().join("media").join("posts");
    fs::create_dir_all(&posts).expect("the fixture must hold");
    for index in 0..10 {
        fs::write(posts.join(format!("{index}.jpg")), format!("post-{index}")).expect("the fixture must hold");
    }
    // A file outside the posts folder that must not be measured: a profile picture is not the
    // photographer's work.
    fs::write(dir.path().join("profile.jpg"), b"avatar").expect("the fixture must hold");

    let reference = source::resolve(
        "@somebody",
        MediaSource::InstagramExport,
        Some(dir.path()),
    )
    .expect("the fixture must hold");

    assert_eq!(reference.len(), 10, "the walk left the posts folder");
    assert!(reference
        .reasons
        .iter()
        .any(|reason| reason.code
            == aura_core::contract::look::LookCode::InstagramExportLayout));
    assert!(reference
        .reasons
        .iter()
        .any(|reason| reason.code
            == aura_core::contract::look::LookCode::OriginRecordedNotFetched));
}

#[test]
fn an_export_whose_layout_has_moved_still_finds_the_photographs() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    let deep = dir.path().join("something").join("unexpected");
    fs::create_dir_all(&deep).expect("the fixture must hold");
    for index in 0..10 {
        fs::write(deep.join(format!("{index}.jpg")), format!("post-{index}")).expect("the fixture must hold");
    }

    let reference = source::resolve("", MediaSource::InstagramExport, Some(dir.path())).expect("the fixture must hold");

    assert_eq!(reference.len(), 10);
}

#[test]
fn the_page_is_remembered_even_though_the_files_came_from_a_folder() {
    let dir = tempfile::tempdir().expect("the fixture must hold");
    for index in 0..10 {
        fs::write(dir.path().join(format!("{index}.jpg")), format!("{index}")).expect("the fixture must hold");
    }

    let reference = source::resolve(
        "https://instagram.com/the.photographer",
        MediaSource::Folder,
        Some(dir.path()),
    )
    .expect("the fixture must hold");

    assert_eq!(
        reference.origin,
        ReferenceOrigin::Instagram {
            handle: "the.photographer".to_string()
        },
        "the page a look is from must survive the files arriving another way"
    );
    assert_eq!(reference.source, MediaSource::Folder);
}
