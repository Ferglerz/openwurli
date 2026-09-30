fn main() {
    println!("cargo:rerun-if-changed=reference/src");
    println!("cargo:rerun-if-changed=reference/Cargo.toml");
    println!("cargo:rerun-if-changed=reference-manifest.json");
    println!("cargo:rerun-if-changed=reference.py");
    let result = std::process::Command::new("python3")
        .arg("reference.py")
        .status()
        .expect("python3 required for immutable reference verification");
    assert!(result.success(), "reference drift detected");
}
