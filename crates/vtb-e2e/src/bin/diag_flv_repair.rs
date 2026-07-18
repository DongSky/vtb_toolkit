//! Diagnostic: FLV timestamp repair on a real recording.
//!
//! Usage: cargo run -p vtb-e2e --bin diag_flv_repair -- <file.flv> [--dry-run]
//! Dry-run scans and reports discontinuities without touching the file.

fn main() {
    let path = std::env::args().nth(1).unwrap_or_default();
    let dry = std::env::args().any(|a| a == "--dry-run");
    if path.is_empty() {
        eprintln!("usage: diag_flv_repair <file.flv> [--dry-run]");
        std::process::exit(2);
    }
    let p = std::path::Path::new(&path);
    let cfg = vtb_recorder::flv::RepairConfig::default();

    let stats = if dry {
        struct Null;
        impl std::io::Write for Null {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut input = std::io::BufReader::new(std::fs::File::open(p).unwrap());
        vtb_recorder::flv::repair_stream(&mut input, &mut Null, &cfg).unwrap()
    } else {
        vtb_recorder::flv::repair_file(p, &cfg).unwrap()
    };

    println!(
        "tags={} discontinuities={} duration={:.1}s truncated={}{}",
        stats.tags,
        stats.discontinuities,
        stats.last_timestamp_ms as f64 / 1000.0,
        stats.truncated,
        if dry {
            " (dry-run)"
        } else if stats.discontinuities > 0 {
            " → 已修复，原文件保留为 .orig"
        } else {
            " → 时间轴正常，未改动"
        }
    );
}
