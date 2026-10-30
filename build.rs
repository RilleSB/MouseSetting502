use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR missing"));
        let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("icon.ico");
        let rc = dir.join("mouse-settings-icon.rc");
        let escaped = icon.to_string_lossy().replace('\\', "/").replace('"', "\\\"");
        fs::write(&rc, format!("1 ICON \"{escaped}\"\n")).expect("Cannot write icon resource");
        let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        let out = dir.join(if msvc { "mouse-settings-icon.res" } else { "mouse-settings-icon.o" });
        let status = if msvc {
            Command::new("rc.exe").arg("/nologo").arg(format!("/fo{}", out.display())).arg(&rc).status()
        } else {
            // Some machines have an incomplete GCC earlier in PATH. Select a
            // resource compiler next to a GCC that can find its preprocessor.
            let paths = env::var_os("PATH").unwrap_or_default();
            let tool_dir = env::split_paths(&paths).find(|dir| {
                let gcc = dir.join("gcc.exe");
                dir.join("windres.exe").is_file() && gcc.is_file()
                    && Command::new(gcc).arg("-print-prog-name=cc1").output().ok()
                        .filter(|out| out.status.success())
                        .is_some_and(|out| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).is_file())
            });
            let windres = env::var_os("WINDRES").map(PathBuf::from)
                .or_else(|| tool_dir.as_ref().map(|dir| dir.join("windres.exe")))
                .unwrap_or_else(|| PathBuf::from("windres"));
            let mut command = Command::new(&windres);
            if let Some(dir) = windres.parent().filter(|dir| !dir.as_os_str().is_empty()) {
                let search = env::join_paths(std::iter::once(dir.to_path_buf()).chain(env::split_paths(&paths)))
                    .expect("Invalid compiler search path");
                command.env("PATH", search);
            }
            command.arg("-i").arg(&rc).arg("-o").arg(&out).arg("-O").arg("coff").status()
        }.expect("Windows resource compiler not found");
        assert!(status.success(), "Failed to embed icon.ico in the executable");
        println!("cargo:rustc-link-arg-bin=ghub_rust={}", out.display());
    }
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint build failed");
}
