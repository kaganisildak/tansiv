use flatc_rust;
use std::env;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let out_dir = env::var("OUT_DIR")
        .expect("OUT_DIR environment variable is not defined");
    let packets_def = "../../wire/packets.fbs";
    flatc_rust::run(flatc_rust::Args {
        lang: "rust",  // `rust` is the default, but let's be explicit
        inputs: &[Path::new(packets_def)],
        out_dir: &Path::new(&out_dir),
        ..Default::default()
    })?;

    println!("cargo:rerun-if-changed={}", packets_def);

    Ok(())
}
