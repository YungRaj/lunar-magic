#![cfg(target_os = "macos")]

use lm_project::Project;
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn snes9x_binary() -> PathBuf {
    std::env::var_os("SNES9X_BIN").map_or_else(
        || PathBuf::from("/Applications/Snes9x.app/Contents/MacOS/Snes9x"),
        PathBuf::from,
    )
}

fn source_rom(root: &Path) -> PathBuf {
    root.join("Super Mario World (USA).sfc")
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_expanded_rom_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = snes9x_binary();
    assert!(
        snes9x.is_file(),
        "Snes9x executable is missing: {}",
        snes9x.display()
    );

    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .expect("expand and checksum generated ROM");

    let directory = std::env::temp_dir().join(format!(
        "lm-snes9x-smoke-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).expect("create Snes9x smoke directory");
    let output = directory.join("Rust-generated-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write generated ROM");

    let child = Command::new(&snes9x)
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Snes9x");
    let mut child = ChildGuard(child);
    thread::sleep(Duration::from_secs(8));
    assert!(
        child.0.try_wait().expect("query Snes9x process").is_none(),
        "Snes9x exited during generated-ROM initialization"
    );

    child.0.kill().expect("stop Snes9x smoke process");
    child.0.wait().expect("reap Snes9x smoke process");
    fs::remove_dir_all(directory).expect("remove Snes9x smoke directory");
}
