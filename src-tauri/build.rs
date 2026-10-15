fn main() {
    println!("cargo:rerun-if-changed=resources");
    if std::env::var("PROFILE").is_ok_and(|value| value == "release") {
        for file in [
            "adb.exe",
            "AdbWinApi.dll",
            "AdbWinUsbApi.dll",
            "NOTICE.txt",
            "manifest.json",
        ] {
            assert!(
                std::path::Path::new("resources/adb").join(file).is_file(),
                "Missing ADB resource {file}; run pnpm prepare:adb before release build"
            );
        }
    }
    println!("cargo:rerun-if-changed=native-runtime");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    for file in [
        "opencv_world4120.dll",
        "msvcp140.dll",
        "concrt140.dll",
        "vcruntime140.dll",
        "vcruntime140_1.dll",
    ] {
        let source = std::path::Path::new("native-runtime").join(file);
        assert!(source.is_file(), "Missing {file}; run pnpm prepare:opencv");
        for folder in [
            profile.to_path_buf(),
            profile.join("deps"),
            profile.join("examples"),
        ] {
            std::fs::create_dir_all(&folder).expect("runtime folder");
            let destination = folder.join(file);
            // Windows keeps loaded DLLs locked. An identical staged runtime needs no write.
            if std::fs::read(&destination).ok() != std::fs::read(&source).ok() {
                std::fs::copy(&source, &destination).expect("stage native runtime beside executable");
            }
        }
    }
    tauri_build::build()
}
