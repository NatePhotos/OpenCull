//! Read-only native inference smoke test: OPENCULL_AI=cuda cargo run --features cuda --example ai_backend -- photo.jpg
use opencull_lib::{ai, preview};
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("usage: ai_backend <photo.jpg or RAW>")?;
    let image = preview::decode_upright(std::path::Path::new(&path), u32::MAX)?;
    for iteration in 1..=3 {
        let start = std::time::Instant::now();
        let result = ai::analyze_pixels(&image.rgb, image.width, image.height, ai::Rules::Advanced)?;
        println!("run={iteration} faces={} analysis_ms={:.1} cuda_enabled={} fallback={:?}", result.faces.len(), start.elapsed().as_secs_f64()*1000.0, ai::acceleration::enabled(), ai::acceleration::failure());
    }
    Ok(())
}
