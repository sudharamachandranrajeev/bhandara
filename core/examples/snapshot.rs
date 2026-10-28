//! Print volumes, one throughput sample and home folders as JSON.
//! Run: cargo run -p bhandara-core --example snapshot

fn main() -> std::io::Result<()> {
    let volumes = bhandara_core::volumes()?;
    let knames: Vec<String> = volumes.iter().map(|v| v.kname.clone()).collect();
    let mut throughput = bhandara_core::Throughput::new();
    throughput.poll(&knames);
    std::thread::sleep(std::time::Duration::from_secs(1));
    let rates = throughput.poll(&knames);
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    let folders = bhandara_core::largest_children(&home, 5, |_, _| {})?;
    let out = serde_json::json!({ "volumes": volumes, "rates": rates, "home": folders });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    Ok(())
}
