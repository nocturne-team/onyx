//! Imports an exported Prism Launcher instance (`.zip`) into an Onyx
//! instances folder and prints the new instance's folder:
//!
//! `cargo run -p onyx-core --example prism_import -- GTNH.zip /tmp/instances`

use std::sync::atomic::AtomicBool;

use onyx_core::download::Progress;
use onyx_core::prism::archive;

fn main() {
    let mut args = std::env::args().skip(1);
    let zip = args.next().expect("pass a .zip");
    let root = args.next().expect("pass an instances folder");
    let found = archive::inspect(zip.as_ref()).unwrap_or_else(|e| panic!("{e}"));
    if let Some(reason) = &found.instance.unsupported {
        panic!("can't import: {reason}");
    }
    let created = archive::create(root.as_ref(), &found).unwrap_or_else(|e| panic!("{e}"));
    let dir = std::path::Path::new(&root).join(&created.id);
    let started = std::time::Instant::now();
    archive::extract_game_folder(&found, &dir, &Progress::default(), &AtomicBool::new(false))
        .unwrap_or_else(|e| panic!("{e}"));
    eprintln!("extracted in {:?}", started.elapsed());
    println!("{}", dir.display());
}
