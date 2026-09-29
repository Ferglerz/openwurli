// TEMPORARY: DELETE AFTER CPU/AUDIO STUDY. Historical source stays in git.
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut modules = String::new();
    for (name, variable, default_ref) in [
        (
            "scalar",
            "REED_SCALAR_REF",
            "6614ab9519471e956ecb0c82edc4fcf295f724dc",
        ),
        (
            "simd",
            "REED_SIMD_REF",
            "612dbda54cad9a8a666aa5dc6496d73f5d5da27d",
        ),
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
        let reference = env::var(variable).unwrap_or_else(|_| default_ref.to_owned());
        let object = format!("{reference}:crates/openwurli-dsp/src/reed.rs");
        let result = Command::new("git")
            .arg("show")
            .arg(&object)
            .current_dir(&root)
            .output()
            .expect("git must be available for historical comparison");
        assert!(
            result.status.success(),
            "Missing historical source {object}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let path = out.join(format!("{name}.rs"));
        fs::write(&path, result.stdout).unwrap();
        modules.push_str(&format!("#[path = {:?}] mod {name};\n", path));
    }
    fs::write(out.join("historical_modules.rs"), modules).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
