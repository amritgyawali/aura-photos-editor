//! The phase 30 delivery gates. PHASE-30 section 10.1.
//!
//! Run as an ordinary test so a red gate is a red build, beside the phase 05 to 29 harnesses.
//!
//! ## What these gates prove, and what they do not
//!
//! Section 10.1 asks ten things of this phase. Four of them are **not measurable here** and are
//! named rather than approximated:
//!
//! - *"Lightroom shows AURA's selections and grading"* needs Lightroom. What runs instead is a
//!   round trip through the XMP writer and reader this repository owns, which proves the document
//!   is what the plugin's parser expects and proves nothing about Adobe's.
//! - *"the learning loop improves style match by 15 % after 3 corrected weddings"* needs a
//!   photographer's archive. What runs is the same measurement on corrections this repository
//!   authored, which proves the fit, the split and the bound.
//! - *"installers are signed and notarised"* needs a signing service and two operating systems.
//! - *"crash-free session rate ≥ 99.5 %"* needs a closed beta with twenty photographers.
//!
//! Those are conditions C3 to C6 of `docs/progress/PHASE-30-EXIT.md`.
//!
//! What *is* measured here is the half of the phase that can be: the write and its read-back, the
//! naming across four thousand files, the resize and sharpen arithmetic, the three writers'
//! fidelity, the manifest's shape, the resume protocol, the aggregation and the two bounds, and
//! every refusal.
//!
//! ## Why gate 1 decodes
//!
//! "Rendered JPEG matches the reference render within a perceptual tolerance" is a question about
//! **pixels**, not about bytes. A test that encoded a JPEG and checked its length would prove the
//! encoder ran. So gate 1 decodes what was written back and compares it to the buffer that went in,
//! which is the same discipline phase 29's monochrome gate takes: a guarantee about a pixel is
//! measured on the pixel.

use std::collections::BTreeSet;
use std::path::PathBuf;

use aura_core::contract::delivery::{
    DeliveryCode, DeliveryColour, Destination, ExportJob, ExportSet, FileFormat, ImageId,
    MetadataPolicy, NamingTemplate, OutputSharpen, Resize, UploadState, MAX_REASONS,
    MIN_JPEG_QUALITY, MIN_LONG_EDGE,
};
use aura_core::contract::ids::DecisionId;
use aura_core::contract::learn::{
    CorrectionBucket, Learnable, MIN_CORRECTIONS, MIN_OFFERABLE_IMPROVEMENT, MIN_PROJECTS,
};
use aura_core::contract::ledger::DecisionKind;
use aura_core::contract::scene::SceneId;
use aura_export::fixtures::{plate, Plate, ScriptedField};
use aura_export::read::{Frame, Samples};
use aura_export::verify::{hash_file, write_and_verify};
use aura_export::{jpeg, manifest, naming, png, resample, tiff};

// ---------------------------------------------------------------------------
// Gate 1. Export fidelity: what comes out of the file is what went into it.
// ---------------------------------------------------------------------------

/// The largest per-channel error a lossy encode may introduce at delivery quality.
///
/// Six code values out of 255. That is the DCT's own rounding at quality 92 with no chroma
/// subsampling; anything above it is a bug in the encoder's configuration rather than in JPEG.
const JPEG_TOLERANCE: i32 = 6;

#[test]
fn gate_1_a_written_jpeg_decodes_back_to_the_photograph() {
    for kind in [Plate::Gradient, Plate::Edge, Plate::Primaries, Plate::Flat] {
        let src = plate(kind, 96, 72, DeliveryColour::Srgb, 8);
        let (bytes, reasons) = jpeg::encode(&src, 92, &MetadataPolicy::default()).expect("encode");

        let mut decoder = zune_jpeg::JpegDecoder::new(&bytes[..]);
        let pixels = decoder.decode().expect("decode");
        let info = decoder.info().expect("info");
        assert_eq!((info.width, info.height), (96, 72), "{kind:?}");

        let Samples::Eight(original) = &src.data else {
            panic!("eight bit")
        };
        assert_eq!(pixels.len(), original.len(), "{kind:?}");
        let worst = original
            .iter()
            .zip(pixels.iter())
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .max()
            .unwrap_or(0);
        assert!(
            worst <= JPEG_TOLERANCE,
            "{kind:?}: worst channel error {worst}, above {JPEG_TOLERANCE}"
        );

        // ICC and the metadata policy's own notes travel with it.
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::GpsStripped));
    }
}

#[test]
fn gate_1_a_written_tiff_carries_the_exact_samples_and_a_profile() {
    // TIFF is lossless, so "within a tolerance" is the wrong test: the samples have to be
    // identical, and a writer that was off by one would be a writer nobody could trust with a
    // sixteen-bit album file.
    for bits in [8_u8, 16] {
        let src = plate(Plate::Gradient, 32, 24, DeliveryColour::AdobeRgb, bits);
        let (bytes, reasons) = tiff::encode(&src, &MetadataPolicy::default()).expect("encode");

        // Walk the IFD the way a reader does, and read the pixels out of the strip it names.
        assert_eq!(&bytes[..2], b"II");
        let ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let count = u16::from_le_bytes([bytes[ifd], bytes[ifd + 1]]) as usize;
        let mut strip_offsets = None;
        let mut bits_per_sample = None;
        let mut icc = None;
        for i in 0..count {
            let at = ifd + 2 + i * 12;
            let tag = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
            let value =
                u32::from_le_bytes([bytes[at + 8], bytes[at + 9], bytes[at + 10], bytes[at + 11]]);
            match tag {
                0x0111 => strip_offsets = Some(value as usize),
                0x0102 => bits_per_sample = Some(value as usize),
                0x8773 => {
                    let len = u32::from_le_bytes([
                        bytes[at + 4],
                        bytes[at + 5],
                        bytes[at + 6],
                        bytes[at + 7],
                    ]);
                    icc = Some((value as usize, len as usize));
                }
                _ => {}
            }
        }

        let bps = bits_per_sample.expect("bits per sample");
        assert_eq!(
            u16::from_le_bytes([bytes[bps], bytes[bps + 1]]),
            u16::from(bits)
        );

        let so = strip_offsets.expect("strip offsets");
        let first =
            u32::from_le_bytes([bytes[so], bytes[so + 1], bytes[so + 2], bytes[so + 3]]) as usize;
        match &src.data {
            Samples::Eight(expected) => {
                assert_eq!(&bytes[first..first + expected.len()], &expected[..]);
            }
            Samples::Sixteen(expected) => {
                for (i, sample) in expected.iter().enumerate() {
                    let at = first + i * 2;
                    assert_eq!(
                        u16::from_le_bytes([bytes[at], bytes[at + 1]]),
                        *sample,
                        "sample {i}"
                    );
                }
            }
        }

        let (icc_at, icc_len) = icc.expect("an icc profile");
        assert!(icc_len > 200);
        assert_eq!(&bytes[icc_at + 36..icc_at + 40], b"acsp");
        assert!(reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
    }
}

#[test]
fn gate_1_a_written_png_round_trips_and_says_what_it_could_not_embed() {
    let src = plate(Plate::Chequer, 24, 24, DeliveryColour::Srgb, 16);
    let (bytes, _) = png::encode(&src, &MetadataPolicy::default()).expect("encode");

    let decoder = ::png::Decoder::new(&bytes[..]);
    let mut reader = decoder.read_info().expect("read info");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("frame");
    assert_eq!((info.width, info.height), (24, 24));
    assert_eq!(info.bit_depth, ::png::BitDepth::Sixteen);

    let Samples::Sixteen(expected) = &src.data else {
        panic!("sixteen bit")
    };
    // PNG is big endian for sixteen-bit samples, which is the opposite of TIFF and the single most
    // common way a writer produces a file that decodes to noise.
    for (i, sample) in expected.iter().enumerate().take(64) {
        let at = i * 2;
        assert_eq!(
            u16::from_be_bytes([buf[at], buf[at + 1]]),
            *sample,
            "sample {i}"
        );
    }

    // A non-sRGB PNG says it carries chromaticities rather than claiming a profile it did not
    // write. Phase 24's rule: an absent capability and a satisfied one must not render the same.
    let adobe = plate(Plate::Flat, 8, 8, DeliveryColour::AdobeRgb, 8);
    let (_, reasons) = png::encode(&adobe, &MetadataPolicy::default()).expect("encode");
    assert!(reasons
        .iter()
        .any(|r| r.code == DeliveryCode::IccUnavailable));
    assert!(!reasons.iter().any(|r| r.code == DeliveryCode::IccEmbedded));
}

// ---------------------------------------------------------------------------
// Gate 2. Verification catches a deliberately corrupted write.
// ---------------------------------------------------------------------------

#[test]
fn gate_2_verification_catches_a_corrupted_write_and_a_truncated_one() {
    let dir = tempfile::tempdir().expect("tempdir");

    // A flipped byte, which is what a bad sector produces.
    let path = dir.path().join("flip.bin");
    let bytes = vec![9_u8; 8192];
    let written = write_and_verify(&path, &bytes, true).expect("write");
    assert!(written.verified);
    assert_eq!(written.hash, blake3::hash(&bytes).to_hex().to_string());

    let mut on_disk = std::fs::read(&path).expect("read");
    on_disk[4096] ^= 0x01;
    std::fs::write(&path, &on_disk).expect("corrupt");
    assert_ne!(hash_file(&path).expect("hash"), written.hash);

    // A truncation, which is what a full volume produces.
    let path = dir.path().join("trunc.bin");
    let written = write_and_verify(&path, &bytes, true).expect("write");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open");
    file.set_len(4096).expect("truncate");
    file.sync_all().expect("sync");
    drop(file);
    assert_eq!(std::fs::metadata(&path).expect("stat").len(), 4096);
    assert_ne!(hash_file(&path).expect("hash"), written.hash);

    // And a job that asked for no verification carries **no digest at all**, rather than a
    // plausible one. The schema refuses the combination that would make a manifest a lie.
    let path = dir.path().join("unverified.bin");
    let written = write_and_verify(&path, &bytes, false).expect("write");
    assert!(!written.verified);
    assert!(written.hash.is_empty());
}

// ---------------------------------------------------------------------------
// Gate 3. Naming templates produce collision-free names across 4,000 files,
//         including duplicate original names from two cameras.
// ---------------------------------------------------------------------------

#[test]
fn gate_3_four_thousand_frames_from_two_cameras_get_four_thousand_names() {
    let count = 4000;
    let images: Vec<ImageId> = (0..count).map(|_| ImageId::new()).collect();
    let mut field = ScriptedField::new(Some("Álex & Sam O'Neill"), count as u32, count as u32);
    for (ix, image) in images.iter().enumerate() {
        field = field.with_frame(
            *image,
            Frame {
                image: Some(*image),
                // Two Nikons, one wedding: the same twelve stems over and over. That is not an
                // edge case, it is what happens whenever two bodies of one make share a day.
                original_stem: Some(format!("DSC_{:04}", ix % 12)),
                date: Some("2026-05-16".to_owned()),
                ..Frame::default()
            },
        );
    }

    let job = ExportJob::new(
        vec![ExportSet {
            name: "gallery".to_owned(),
            images: images.clone(),
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::None,
            naming: NamingTemplate::parse("{original}").expect("template"),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        }],
        Destination::Folder {
            path: PathBuf::from("/x"),
        },
    );

    let planned = naming::plan(&job, &field).expect("plan");
    assert_eq!(planned.len(), count);

    // Case-insensitively unique, because two of the three filesystems this product runs on are and
    // a gallery that collides only on Windows arrives broken at the client.
    let unique: BTreeSet<String> = planned
        .iter()
        .map(|p| p.rel_path.to_string_lossy().to_ascii_lowercase())
        .collect();
    assert_eq!(unique.len(), count, "names collided");

    // The collisions were resolved rather than hidden, and the panel is told.
    let renamed = planned.iter().filter(|p| p.renamed).count();
    assert_eq!(renamed, count - 12, "twelve originals, the rest suffixed");
    assert!(planned.iter().any(|p| p
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::NameCollisionResolved)));

    // And the couple's name survived a filesystem no matter what they are called.
    let couple = naming::slugify("Álex & Sam O'Neill");
    assert_eq!(couple, "alex-and-sam-oneill");
}

#[test]
fn gate_3_a_template_that_cannot_distinguish_says_so_before_the_job_runs() {
    let flat = NamingTemplate::parse("{date}_{couple}").expect("template");
    assert!(!flat.is_distinguishing());
    let good = NamingTemplate::parse("{date}_{seq}").expect("template");
    assert!(good.is_distinguishing());

    // And a template that could write outside the destination is refused at parse time.
    for bad in [
        "{date}/{seq}",
        "../{seq}",
        "C:{seq}",
        "{venue}_{seq}",
        "{seq",
    ] {
        assert!(NamingTemplate::parse(bad).is_err(), "`{bad}` was accepted");
    }
}

// ---------------------------------------------------------------------------
// Gate 4. The resize and sharpen arithmetic, which is where a delivery gets muddy.
// ---------------------------------------------------------------------------

#[test]
fn gate_4_a_downscale_in_linear_light_does_not_darken_a_fine_texture() {
    // The defect this gate exists for. A one-pixel chequer averages to a *linear* half, which
    // encodes to about 188 in sRGB - not 128. A resizer that produced 128 has darkened every lace
    // veil, beaded sari and backlit hair in the wedding by nearly a stop, and it compounds with
    // contrast.
    let src = plate(Plate::Chequer, 128, 128, DeliveryColour::Srgb, 8);
    let out = resample::downscale(&src, 16, 16);
    let mid = out.data.unit(3 * (8 * 16 + 8)).expect("sample") * 255.0;
    assert!(
        (180.0..195.0).contains(&mid),
        "a linear-light average of black and white encodes near 188, got {mid}"
    );
}

#[test]
fn gate_4_a_stripe_at_the_nyquist_frequency_does_not_beat() {
    // Why the filter is a triangle rather than a box: a box aliases, and a striped suit at 4:1 is
    // moiré that no amount of output sharpening removes.
    let src = plate(Plate::Chequer, 256, 8, DeliveryColour::Srgb, 8);
    let out = resample::downscale(&src, 64, 2);
    let row: Vec<f32> = (0..64)
        .map(|x| out.data.unit(x * 3).expect("sample"))
        .collect();
    let lo = row.iter().copied().fold(f32::MAX, f32::min);
    let hi = row.iter().copied().fold(f32::MIN, f32::max);
    assert!(hi - lo < 0.05, "moiré: {} across the row", hi - lo);
}

#[test]
fn gate_4_output_sharpening_grows_as_a_frame_is_scaled_down() {
    // Resolution-aware, which is section 6.1's word. A frame scaled to a quarter has lost its
    // acutance to the resampler and needs more than one at full size.
    let full = OutputSharpen::Screen.amount(1.0);
    let web = OutputSharpen::Screen.amount(0.24);
    assert!(web > full);
    assert!(OutputSharpen::Print.amount(1.0) > OutputSharpen::Screen.amount(1.0));
    assert_eq!(OutputSharpen::None.amount(0.2), 0.0);

    // And it leaves a flat field alone, which is what the threshold is for: an unthresholded mask
    // puts a visible step in a sky.
    let flat = plate(Plate::Flat, 32, 32, DeliveryColour::Srgb, 8);
    let sharp = resample::sharpen(&flat, OutputSharpen::Screen, 0.4);
    assert_eq!(sharp.data, flat.data);
}

#[test]
fn gate_4_nothing_is_ever_upscaled() {
    let src = plate(Plate::Flat, 200, 150, DeliveryColour::Srgb, 8);
    let out = resample::downscale(&src, 4000, 3000);
    assert_eq!((out.width, out.height), (200, 150));

    let r = Resize::LongEdge { pixels: 8000 };
    assert!(r.would_upscale(200, 150));
    assert_eq!(r.target(200, 150), (200, 150));
}

#[test]
fn gate_4_the_transfer_curves_round_trip_on_every_code_value() {
    for space in DeliveryColour::ALL {
        for i in 0_u8..=255 {
            let v = f32::from(i) / 255.0;
            let back = resample::from_linear(space, resample::to_linear(space, v));
            assert!((back - v).abs() < 1e-4, "{space:?} at {v}: {back}");
        }
    }
}

// ---------------------------------------------------------------------------
// Gate 5. The manifest is a document another tool can read.
// ---------------------------------------------------------------------------

#[test]
fn gate_5_the_manifest_parses_and_carries_what_a_delivery_needs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = aura_core::ProjectId::new();
    let files: Vec<aura_core::contract::delivery::ExportedFile> = (0..3)
        .map(|i| {
            let bytes = vec![i as u8; 128];
            aura_core::contract::delivery::ExportedFile {
                image: ImageId::new(),
                set: "gallery".to_owned(),
                path: PathBuf::from("gallery").join(format!("{i:04}.jpg")),
                bytes: bytes.len() as u64,
                hash: blake3::hash(&bytes).to_hex().to_string(),
                width: 100,
                height: 100,
                render_hash: "a".repeat(64),
                verified: true,
                renamed: false,
                reasons: vec![aura_core::contract::delivery::DeliveryReason::with(
                    DeliveryCode::CleanupDisclosed,
                    "an exit sign was removed from the background",
                )],
            }
        })
        .collect();

    let m = manifest::assemble(
        project,
        1_760_000_000_000,
        &files,
        &[("gallery".to_owned(), 3)],
        Some(PathBuf::from("qc/report.json")),
        vec![("app".to_owned(), "0.1.0".to_owned())],
    );
    assert!(m.fully_hashed());
    // Every removal reaches the document handed to the client. A removal that is not disclosed
    // there is a removal nobody can audit.
    assert_eq!(m.cleanup_disclosures.len(), 3);

    let (path, hash) = manifest::seal(dir.path(), &m, true).expect("seal");
    assert_eq!(hash, hash_file(&path).expect("hash"));

    let text = std::fs::read_to_string(&path).expect("read");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed["schema"], "aura.delivery-manifest/1");
    assert_eq!(parsed["file_count"], 3);
    assert_eq!(parsed["verified"], true);
    assert_eq!(parsed["qc_report"], "qc/report.json");
    assert_eq!(parsed["engine_versions"]["app"], "0.1.0");

    // Two identical deliveries produce identical documents, which is what makes the stored digest
    // a check on the document rather than a timestamp of it.
    let again = manifest::to_document(&m, true);
    assert_eq!(again, text);
}

// ---------------------------------------------------------------------------
// Gate 6. Provider uploads resume after a network drop, and per-set mapping is respected.
// ---------------------------------------------------------------------------

#[test]
fn gate_6_an_upload_resumes_from_what_the_far_end_kept() {
    use aura_delivery::providers::{registry, ScriptedTransport};
    use aura_delivery::resume;

    let provider = registry("folder-gallery").expect("provider");
    let transport = ScriptedTransport::new();
    let bytes: Vec<u8> = (0..(resume::CHUNK * 3 + 917))
        .map(|i| (i % 251) as u8)
        .collect();
    let item = aura_core::contract::delivery::UploadItem {
        image: ImageId::new(),
        set: "gallery".to_owned(),
        path: PathBuf::from("gallery/0001.jpg"),
        bytes: bytes.len() as u64,
        hash: blake3::hash(&bytes).to_hex().to_string(),
        state: UploadState::Pending,
    };
    let mapping = aura_core::contract::delivery::SetMapping {
        set: "gallery".to_owned(),
        remote: "wedding-2026".to_owned(),
        publish: false,
    };
    let key = provider.key_for(&mapping, &item.path);
    assert_eq!(key, "wedding-2026/0001.jpg");

    transport.drop_after(resume::CHUNK / 3);
    let first = resume::step(&transport, &item, &bytes, &key);
    let UploadState::InProgress { sent, .. } = first.state else {
        panic!("a drop left the file {:?}", first.state)
    };
    assert!(
        sent > 0,
        "the far end kept nothing, so a resume is a restart"
    );

    transport.recover();
    let mut resumed = item.clone();
    resumed.state = first.state;
    let second = resume::send(&transport, &resumed, &bytes, &key).expect("resume");
    assert_eq!(second.state, UploadState::Verified);
    assert!(
        second.sent < item.bytes,
        "the resume re-sent the whole file"
    );
    assert_eq!(transport.contents(&key), Some(bytes));
    assert!(second
        .reasons
        .iter()
        .any(|r| r.code == DeliveryCode::UploadResumed));
}

#[test]
fn gate_6_the_offset_comes_from_the_far_end_and_not_from_the_stored_row() {
    // A stored offset is a claim about somebody else's disk, and the two disagree exactly when it
    // matters - after a crash mid-put. Resuming from the local number leaves a hole in the middle
    // of a photograph, and only the digest at the end would notice.
    use aura_delivery::providers::{ScriptedTransport, Transport};
    use aura_delivery::resume;

    let transport = ScriptedTransport::new();
    let bytes: Vec<u8> = (0..2000).map(|i| (i % 251) as u8).collect();
    let mut item = aura_core::contract::delivery::UploadItem {
        image: ImageId::new(),
        set: "gallery".to_owned(),
        path: PathBuf::from("a.jpg"),
        bytes: bytes.len() as u64,
        hash: blake3::hash(&bytes).to_hex().to_string(),
        state: UploadState::Pending,
    };
    transport.put("k", 0, &bytes[..700]).expect("partial");
    item.state = UploadState::InProgress {
        sent: 1900,
        resumes: 1,
    };
    let done = resume::send(&transport, &item, &bytes, "k").expect("send");
    assert_eq!(done.state, UploadState::Verified);
    assert_eq!(transport.contents("k"), Some(bytes), "a hole in the file");
}

#[test]
fn gate_6_an_unmapped_set_is_left_out_and_named() {
    use aura_delivery::mapping::Mapping;

    let mapping = Mapping::new(&[aura_core::contract::delivery::SetMapping {
        set: "gallery".to_owned(),
        remote: "main".to_owned(),
        publish: true,
    }]);
    let sets = vec!["gallery".to_owned(), "album".to_owned()];
    let (mapped, unmapped) = mapping.split(&sets);
    assert_eq!(mapped.len(), 1);
    assert_eq!(unmapped.len(), 1);
    assert_eq!(unmapped[0].1.code, DeliveryCode::SetUnmapped);

    // And a publish flag is cleared with a note rather than silently. A photographer who ticked
    // publish and was quietly not published thinks their client has the gallery.
    let (cleared, notes) = mapping.without_publish();
    assert!(!cleared.wants_publish());
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].code, DeliveryCode::LeftUnpublished);
}

// ---------------------------------------------------------------------------
// Gate 7. The learning loop improves on corrections the fit never saw - measured on
//         authored corrections, which is condition C4.
// ---------------------------------------------------------------------------

#[test]
fn gate_7_a_candidate_improves_on_held_out_corrections_and_is_bounded_twice() {
    use aura_learn::aggregate::{fold, Sample};
    use aura_learn::update::{compute, Offsets};

    let bucket = CorrectionBucket {
        kind: DecisionKind::Edit,
        scene: SceneId::Unknown,
        learnable: Learnable::Exposure,
        subject_close: false,
    };
    // Three weddings, twenty corrections each, all saying the same thing. Section 10.1's "after 3
    // corrected weddings", with the caveat that these are corrections this repository authored.
    let samples: Vec<Sample> = (0..60)
        .map(|i| Sample {
            decision: DecisionId::new(),
            project: (i % 3) as u64,
            magnitude: 0.30,
        })
        .collect();
    let (agg, _) = fold(bucket, &samples);
    assert!(agg.actionable);
    assert!(agg.corrections >= MIN_CORRECTIONS);
    assert!(agg.projects >= MIN_PROJECTS);

    let candidate = compute(
        aura_core::contract::ids::ProfileId::new(),
        1,
        &Offsets::new(),
        &[(agg, samples)],
    )
    .expect("a candidate");

    assert!(!candidate.update.adopted, "computing adopts nothing");
    assert!(
        candidate.update.expected_improvement >= MIN_OFFERABLE_IMPROVEMENT,
        "improvement {} below the noise floor",
        candidate.update.expected_improvement
    );
    assert!(candidate.comparison.candidate_error < candidate.comparison.current_error);
    assert!(candidate.comparison.held_out > 0, "measured on nothing");

    // Half the measured shift, which is the first bound.
    let moved = candidate.offsets[&(Learnable::Exposure, SceneId::Unknown)];
    assert!((moved - 0.15).abs() < 1e-3, "moved {moved}");
}

#[test]
fn gate_7_one_wedding_is_never_enough_however_many_corrections_it_carries() {
    use aura_learn::aggregate::{fold, Sample};

    let bucket = CorrectionBucket {
        kind: DecisionKind::Edit,
        scene: SceneId::Unknown,
        learnable: Learnable::TemperatureK,
        subject_close: false,
    };
    let samples: Vec<Sample> = (0..200)
        .map(|_| Sample {
            decision: DecisionId::new(),
            project: 0,
            magnitude: 300.0,
        })
        .collect();
    let (agg, reasons) = fold(bucket, &samples);
    assert!(!agg.actionable, "two hundred corrections from one wedding");
    assert_eq!(agg.proposed_offset(), 0.0);
    assert!(reasons
        .iter()
        .any(|r| r.code == aura_core::contract::learn::LearnCode::TooFewWeddings));
}

#[test]
fn gate_7_nothing_a_guarantee_owns_is_learnable() {
    // The closed list. What is not in it is the guarantee: a mask boundary, a retouch texture
    // floor, a crop safety margin, a cleanup permission, an identity cap. A loop that could move
    // one would learn its way past a promise with every gate green.
    for learnable in Learnable::ALL {
        for word in [
            "texture", "identity", "skin", "crop", "cleanup", "mask", "coverage", "tattoo",
        ] {
            assert!(
                !learnable.as_str().contains(word),
                "`{}` names a guarantee",
                learnable.as_str()
            );
        }
        assert!(learnable.ceiling() > 0.0);
    }
    assert_eq!(Learnable::COUNT, 15);
}

// ---------------------------------------------------------------------------
// Gate 8. The refusals: a job that cannot succeed costs nothing.
// ---------------------------------------------------------------------------

#[test]
fn gate_8_every_refusal_happens_before_a_frame_is_rendered() {
    let image = ImageId::new();
    let base = |quality: u8, bits: u8, naming: &str| ExportSet {
        name: "gallery".to_owned(),
        images: vec![image],
        format: FileFormat::Jpeg,
        quality,
        resize: Resize::Full,
        sharpen: OutputSharpen::None,
        naming: NamingTemplate::parse(naming).unwrap_or_default(),
        colour: DeliveryColour::Srgb,
        bit_depth: bits,
        sidecar: false,
    };
    let job = |sets: Vec<ExportSet>| {
        ExportJob::new(
            sets,
            Destination::Folder {
                path: PathBuf::from("/x"),
            },
        )
    };

    // Two sets with one name: their files would land on top of each other and the manifest would
    // report both as delivered.
    assert!(job(vec![base(92, 8, "{seq}"), base(92, 8, "{seq}")])
        .validate()
        .is_err());
    // A quality below the floor.
    assert!(job(vec![base(MIN_JPEG_QUALITY - 1, 8, "{seq}")])
        .validate()
        .is_err());
    // Sixteen bits in a JPEG.
    assert!(job(vec![base(92, 16, "{seq}")]).validate().is_err());
    // No sets at all.
    assert!(job(vec![]).validate().is_err());
    // A legal job passes.
    assert!(job(vec![base(92, 8, "{seq}")]).validate().is_ok());

    // A resize below the floor.
    assert!(Resize::LongEdge {
        pixels: MIN_LONG_EDGE - 1
    }
    .validate()
    .is_err());
    assert!(Resize::LongEdge {
        pixels: MIN_LONG_EDGE
    }
    .validate()
    .is_ok());
}

#[test]
fn gate_8_only_three_codes_stop_a_job_and_every_code_has_a_sentence() {
    let fatal: Vec<DeliveryCode> = DeliveryCode::ALL
        .iter()
        .copied()
        .filter(|c| c.is_fatal())
        .collect();
    assert_eq!(
        fatal,
        vec![
            DeliveryCode::VerificationFailed,
            DeliveryCode::DestinationFull,
            DeliveryCode::DestinationUnwritable
        ]
    );
    for code in DeliveryCode::ALL {
        assert!(!code.user_text().is_empty());
        assert_eq!(DeliveryCode::parse(code.as_str()).expect("parses"), code);
    }
    assert_eq!(MAX_REASONS, 6);
}

// ---------------------------------------------------------------------------
// Gate 9. The metadata policy can only strip.
// ---------------------------------------------------------------------------

#[test]
fn gate_9_no_code_path_writes_a_location_tag() {
    // The strongest form of the guarantee: not "the location is removed" but "there is no code
    // that could write one". 0x8825 is the GPS IFD pointer and it is not in the list of tags this
    // module writes, which a copy-then-remove implementation could never claim.
    assert!(!aura_export::metadata::WRITTEN_TAGS
        .iter()
        .any(|(_, tag)| *tag == 0x8825));
    for (name, _) in aura_export::metadata::WRITTEN_TAGS {
        assert!(!name.to_ascii_lowercase().contains("gps"));
    }

    // Stripping is the default rather than a switch somebody has to find.
    let policy = MetadataPolicy::default();
    assert!(policy.strip_gps);
    assert!(policy.strip_camera_serial);
}
