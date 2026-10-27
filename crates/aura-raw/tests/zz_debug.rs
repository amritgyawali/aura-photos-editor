#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
use aura_raw::container::tiff;
#[test]
#[ignore = "debug"]
fn debug_sony() {
    let path = std::env::var("AURA_PROBE_FILE").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let file = tiff::TiffFile::parse(&bytes).unwrap();
    for ifd in file.ifds() {
        println!("ifd at {} with {} entries", ifd.offset, ifd.entries.len());
    }
    let e = file.find(0xC634);
    println!("c634 {:?} raw {:?}", e, e.and_then(|e| file.raw_u32(e)));
    if let Some(off) = e.and_then(|e| file.raw_u32(e)) {
        let d = file.directory_at(off as usize);
        println!(
            "private {:?}",
            d.as_ref().map(|d| d
                .entries
                .iter()
                .map(|e| (format!("{:#x}", e.tag), e.kind, e.count))
                .collect::<Vec<_>>())
        );
    }
    println!(
        "notes {:?}",
        aura_raw::codecs::sony_notes::read_notes(&file)
    );
}
