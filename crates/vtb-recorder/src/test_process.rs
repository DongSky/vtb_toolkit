//! Native fake recorder for lifecycle tests on Windows as well as Unix.
use crate::FfmpegRecorder;
use std::{path::Path, sync::OnceLock};

pub fn recorder(dir: &Path, write_secs: u32, fail: bool, pcm: bool) -> FfmpegRecorder {
    static BUILD: OnceLock<tempfile::TempDir> = OnceLock::new();
    let build = BUILD.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("fake.rs");
        std::fs::write(&source, r#"
use std::{io::{Read,Write},time::Duration};
fn main() {
    let exe=std::env::current_exe().unwrap();
    let mode=exe.file_stem().unwrap().to_str().unwrap();
    if mode.contains("fail") { std::process::exit(1); }
    let seconds: u32=mode.split('-').nth(1).unwrap().parse().unwrap();
    let args: Vec<String>=std::env::args().collect();
    let out=args.iter().rev().find(|s|s.ends_with(".flv") || s.ends_with(".mkv")).unwrap().replace("%03d","000");
    let pcm=mode.contains("pcm");
    std::thread::spawn(|| {
        for b in std::io::stdin().bytes() { if b.ok()==Some(b'q') { std::process::exit(0); } }
    });
    for _ in 0..seconds {
        let mut file=std::fs::OpenOptions::new().create(true).append(true).open(&out).unwrap();
        file.write_all(b"data\n").unwrap(); file.flush().unwrap();
        if pcm { std::io::stdout().write_all(&[0;16000]).unwrap(); std::io::stdout().flush().unwrap(); }
        std::thread::sleep(Duration::from_secs(1));
    }
    std::thread::sleep(Duration::from_secs(300));
}
"#).unwrap();
        let output=dir.path().join(format!("fake{}", std::env::consts::EXE_SUFFIX));
        let mut command=std::process::Command::new("rustc");
        command.arg(&source).arg("-o").arg(&output);
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
        assert!(command.status().unwrap().success());
        dir
    });
    let binary = dir.join(format!(
        "fake-{write_secs}-{}-{}{}",
        if fail { "fail" } else { "write" },
        if pcm { "pcm" } else { "video" },
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(
        build
            .path()
            .join(format!("fake{}", std::env::consts::EXE_SUFFIX)),
        &binary,
    )
    .unwrap();
    FfmpegRecorder::new(binary)
}
