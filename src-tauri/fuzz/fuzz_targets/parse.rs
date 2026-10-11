//! Every reader of a photo file's bytes, on whatever the fuzzer makes up.
//! A panic, an overflow or a runaway allocation is a finding.
#![no_main]

use libfuzzer_sys::fuzz_target;
use opencull_lib::raw::{self, bmff, io::ReadAt, jpeg};

fuzz_target!(|data: &[u8]| {
    let src: &dyn ReadAt = &data;
    let probe = raw::probe_source(src);
    for e in &probe.embedded {
        assert!(e.offset.checked_add(e.len).is_some_and(|end| end <= data.len() as u64));
    }
    let _ = raw::capture_time_source(src);
    let _ = jpeg::frame(src, 0, src.size());
    let _ = jpeg::layout(src);
    let _ = jpeg::exif_tiff(src, 0, src.size());
    let _ = jpeg::xmp_packet(src, 0, src.size());
    let _ = bmff::scan(src);
    let _ = jpeg::set_orientation(data.to_vec(), 6);
});
