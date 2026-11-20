use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=source/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = format!("{},0", version.replace('.', ","));
    for (binary, description) in [
        ("paymerge-pdf", "PayMerge-PDF Invoice Workspace"),
        ("paymerge-updater", "PayMerge-PDF Update Helper"),
    ] {
        let resource = output.join(format!("{binary}.rc"));
        let object = output.join(format!("{binary}.o"));
        fs::write(
            &resource,
            format!(
                r#"
1 ICON "source/icon.ico"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "{description}\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "ProductName", "PayMerge-PDF\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
            ),
        )
        .unwrap();
        let status = Command::new("x86_64-w64-mingw32-windres")
            .arg("-i")
            .arg(&resource)
            .arg("-o")
            .arg(&object)
            .status()
            .expect("LLVM-MinGW windres must be on PATH; source scripts/env.sh first");
        assert!(status.success(), "Windows resource compilation failed");
        println!("cargo:rustc-link-arg-bin={binary}={}", object.display());
    }
}
