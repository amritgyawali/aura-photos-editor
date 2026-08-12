use aura_core::contract::ids::{ContentHash, PhotoId, ProjectId};
use proptest::array::uniform32;
use proptest::prelude::*;

#[test]
fn db_round_trip() {
    let id = PhotoId::new();
    assert_eq!(PhotoId::from_db(&id.to_db()).expect("parse"), id);
}

#[test]
fn prefixes_are_not_interchangeable() {
    let project = ProjectId::new();
    assert!(
        PhotoId::from_db(&project.to_db()).is_err(),
        "a project id must never parse as a photo id"
    );
}

#[test]
fn v7_ids_sort_in_creation_order() {
    let first = PhotoId::new();
    let second = PhotoId::new();
    assert!(first <= second, "uuid v7 ids must be time-ordered");
}

proptest! {
    #[test]
    fn hash_hex_round_trip(bytes in uniform32(any::<u8>())) {
        let h = ContentHash::from_bytes(bytes);
        prop_assert_eq!(ContentHash::from_hex(&h.to_hex()).expect("parse"), h);
    }
}

#[test]
fn short_or_invalid_hex_is_rejected() {
    assert!(ContentHash::from_hex("abc").is_err());
    assert!(ContentHash::from_hex(&"z".repeat(64)).is_err());
}
