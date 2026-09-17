#![cfg_attr(windows, windows_subsystem = "windows")]
// Compiled only by the opt-in scheduler test, never shipped as an app binary.
use std::{fs::OpenOptions, io::Write, path::PathBuf};
fn main() {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(2)
            .expect("fixture journal argument"),
    );
    for attempt in 1..=8 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(format!("attempt-{attempt}")))
        {
            Ok(mut marker) => {
                writeln!(marker, "{}", std::process::id()).unwrap();
                marker.sync_all().unwrap();
                if attempt == 2 {
                    let started = std::time::Instant::now();
                    while !directory.join("release").exists() && started.elapsed().as_secs() < 90 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
                std::process::exit(3);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("fixture write failed: {error}"),
        }
    }
    std::process::exit(3);
}
