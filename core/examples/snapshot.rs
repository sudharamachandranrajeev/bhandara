//! Print volumes, one throughput sample and an index of a folder (home by
//! default) as JSON, with how long the scan took.
//! Run: cargo run --release -p bhandara-core --example snapshot [folder]

use std::sync::atomic::AtomicBool;

fn main() -> std::io::Result<()> {
    let volumes = bhandara_core::volumes()?;
    let knames: Vec<String> = volumes.iter().map(|v| v.kname.clone()).collect();
    let mut throughput = bhandara_core::Throughput::new();
    throughput.poll(&knames);
    std::thread::sleep(std::time::Duration::from_secs(1));
    let rates = throughput.poll(&knames);

    let root = std::env::args_os()
        .nth(1)
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let started = std::time::Instant::now();
    let index = bhandara_core::index::scan(&root, &[], &AtomicBool::new(false), |_, _| {})?;
    let took = started.elapsed();
    let mut top = index.folder(0).map(|f| f.entries).unwrap_or_default();
    top.truncate(5);
    let types: Vec<_> = index.types().into_iter().take(5).collect();

    let out = serde_json::json!({
        "volumes": volumes,
        "rates": rates,
        "index": { "summary": index.summary(), "seconds": took.as_secs_f64(), "top": top, "types": types },
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    Ok(())
}
