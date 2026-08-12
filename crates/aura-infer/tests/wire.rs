//! The protobuf encoding, read and written.
//!
//! A model file arrives over a network and is then executed, so the reader's
//! failure modes matter as much as its successes: a truncated length prefix must
//! produce `AURA-ML-5010`, never a panic and never a huge allocation.

use aura_infer::onnx::wire::{packed_floats, packed_varints, Reader, WireType, Writer};

#[test]
fn varints_round_trip_across_their_whole_range() {
    for value in [0i64, 1, 127, 128, 300, 65_535, i64::from(u32::MAX), 1 << 40] {
        let mut writer = Writer::new();
        writer.varint(1, value);
        let encoded = writer.finish();

        let mut reader = Reader::new(&encoded);
        let (field, wire) = reader.field().expect("field");
        assert_eq!(field, 1);
        assert_eq!(wire, WireType::Varint);
        assert_eq!(reader.varint().expect("varint") as i64, value);
    }
}

#[test]
fn strings_and_nested_messages_round_trip() {
    let mut inner = Writer::new();
    inner.string(2, "scene_classifier");
    let mut outer = Writer::new();
    outer.message(7, inner);
    let encoded = outer.finish();

    let mut reader = Reader::new(&encoded);
    let (field, wire) = reader.field().expect("field");
    assert_eq!((field, wire), (7, WireType::Bytes));

    let nested = reader.bytes().expect("nested");
    let mut nested_reader = Reader::new(nested);
    let (nested_field, _) = nested_reader.field().expect("nested field");
    assert_eq!(nested_field, 2);
    assert_eq!(nested_reader.string().expect("string"), "scene_classifier");
}

#[test]
fn a_truncated_length_prefix_is_an_error_not_a_panic() {
    let bytes = [0x0a, 0x7f]; // field 1, length 127, no payload
    let mut reader = Reader::new(&bytes);
    let _ = reader.field().expect("field");
    let err = reader.bytes().expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5010");
}

#[test]
fn an_absurd_length_is_refused_before_anything_is_allocated() {
    // Field 1, wire type 2, length 2^40: five continuation bytes and a 0x20.
    let bytes = [0x0a, 0x80, 0x80, 0x80, 0x80, 0x80, 0x20];
    let mut reader = Reader::new(&bytes);
    let _ = reader.field().expect("field");
    let err = reader.bytes().expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5010");
}

#[test]
fn a_varint_longer_than_ten_bytes_is_refused() {
    let bytes = [0x80u8; 12];
    let mut reader = Reader::new(&bytes);
    assert!(reader.varint().is_err());
}

#[test]
fn packed_blocks_refuse_a_partial_value() {
    assert!(packed_floats(&[0, 0, 0]).is_err());
    assert_eq!(
        packed_floats(&[0, 0, 0x80, 0x3f]).expect("one float"),
        [1.0]
    );
    assert_eq!(
        packed_varints(&[0x01, 0x02, 0x03]).expect("three"),
        [1, 2, 3]
    );
}

#[test]
fn unknown_fields_are_skipped_rather_than_refused() {
    // Forward compatibility: a producer that writes doc strings and metadata we
    // do not read must still produce a file we can load.
    let mut writer = Writer::new();
    writer.string(99, "a producer we do not know");
    writer.varint(1, 7);
    let encoded = writer.finish();

    let mut reader = Reader::new(&encoded);
    let (field, wire) = reader.field().expect("field");
    assert_eq!(field, 99);
    reader.skip(wire).expect("skip");

    let (field, _) = reader.field().expect("second field");
    assert_eq!(field, 1);
    assert_eq!(reader.varint().expect("value"), 7);
}

#[test]
fn packed_float_fields_round_trip_through_the_writer() {
    let mut writer = Writer::new();
    writer.packed_floats(7, &[1.0, -2.5, 0.125]);
    let encoded = writer.finish();

    let mut reader = Reader::new(&encoded);
    let (field, wire) = reader.field().expect("field");
    assert_eq!((field, wire), (7, WireType::Bytes));
    assert_eq!(
        packed_floats(reader.bytes().expect("block")).expect("floats"),
        [1.0, -2.5, 0.125]
    );
}
