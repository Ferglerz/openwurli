// Fail closed if an ostensibly shared immutable baseline module changes.
fn main() {
    println!("cargo:rerun-if-changed=../../crates/openwurli-dsp/src");
    println!("cargo:rerun-if-changed=scalar/src");
    println!("cargo:rerun-if-changed=shipping/src");
    println!("cargo:rerun-if-changed=baseline-manifest.json");
    println!("cargo:rerun-if-changed=run.py");
    let result = std::process::Command::new("python3")
        .args(["run.py", "--verify-only"])
        .status()
        .expect("python3 required for baseline verification");
    assert!(
        result.success(),
        "immutable baseline source verification failed"
    );
}
