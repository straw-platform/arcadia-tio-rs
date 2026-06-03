use std::env;
use std::path::{Path, PathBuf};

const LINK_NAME: &str = "arcadia_tio_capi";

fn main() {
    for key in [
        "ARCADIA_TIO_CAPI_LIB_DIR",
        "ARCADIA_TIO_NATIVE_LIB_DIR",
        "ARCADIA_TIO_CAPI_INCLUDE_DIR",
        "ARCADIA_TIO_CAPI_LINK_KIND",
        "ARCADIA_TIO_CAPI_NO_VENDOR",
        "ARCADIA_TIO_CAPI_SYSTEM_FALLBACK",
        "TARGET",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let target = env::var("TARGET").expect("Cargo should set TARGET");
    let link_kind = link_kind();
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo should set CARGO_MANIFEST_DIR"));
    let mut checked = Vec::new();

    if let Some(lib_dir) = explicit_lib_dir() {
        checked.push(format!("explicit lib dir {}", lib_dir.display()));
        validate_dir("native library", &lib_dir);
        warn_if_expected_library_missing(&lib_dir, &target, &link_kind);
        emit_link(&lib_dir, &target, &link_kind);
        emit_include_dir();
        return;
    }

    let no_vendor = env_truthy("ARCADIA_TIO_CAPI_NO_VENDOR");
    let vendored_lib = manifest_dir.join("native").join(&target).join("lib");
    checked.push(format!("vendored lib dir {}", vendored_lib.display()));
    if !no_vendor && vendored_lib.is_dir() {
        warn_if_expected_library_missing(&vendored_lib, &target, &link_kind);
        emit_link(&vendored_lib, &target, &link_kind);
        let vendored_include = manifest_dir.join("native").join(&target).join("include");
        if vendored_include.is_dir() && env::var_os("ARCADIA_TIO_CAPI_INCLUDE_DIR").is_none() {
            emit_resolved_include_dir(&vendored_include);
        } else {
            emit_include_dir();
        }
        return;
    }

    let workspace_vendored_lib = manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("native").join(&target).join("lib"));
    if let Some(root_vendored_lib) = workspace_vendored_lib {
        checked.push(format!(
            "workspace vendored lib dir {}",
            root_vendored_lib.display()
        ));
        if !no_vendor && root_vendored_lib.is_dir() {
            warn_if_expected_library_missing(&root_vendored_lib, &target, &link_kind);
            emit_link(&root_vendored_lib, &target, &link_kind);
            let root_vendored_include = root_vendored_lib
                .parent()
                .expect("native target dir should have a parent")
                .join("include");
            if root_vendored_include.is_dir()
                && env::var_os("ARCADIA_TIO_CAPI_INCLUDE_DIR").is_none()
            {
                emit_resolved_include_dir(&root_vendored_include);
            } else {
                emit_include_dir();
            }
            return;
        }
    }

    if env_truthy("ARCADIA_TIO_CAPI_SYSTEM_FALLBACK") {
        println!("cargo:rustc-link-lib={}={}", link_kind, LINK_NAME);
        emit_include_dir();
        println!(
            "cargo:warning=arcadia-tio-sys using system-library fallback for {LINK_NAME}; runtime lookup remains application/loader-owned"
        );
        return;
    }

    panic!(
        "arcadia-tio-sys could not find native {LINK_NAME} for target {target}. \
         Checked: {}. Set ARCADIA_TIO_CAPI_LIB_DIR (or compatibility alias \
         ARCADIA_TIO_NATIVE_LIB_DIR) to a directory containing the compiled native library, \
         provide native/{target}/lib inside the crate, or opt into a system installation with \
         ARCADIA_TIO_CAPI_SYSTEM_FALLBACK=1. Set ARCADIA_TIO_CAPI_LINK_KIND=dylib|static \
         to choose link kind. Dynamic runtime lookup still requires LD_LIBRARY_PATH, \
         DYLD_LIBRARY_PATH, PATH, rpath, install-name, or equivalent platform loader setup.",
        checked.join(", ")
    );
}

fn explicit_lib_dir() -> Option<PathBuf> {
    env::var_os("ARCADIA_TIO_CAPI_LIB_DIR")
        .or_else(|| env::var_os("ARCADIA_TIO_NATIVE_LIB_DIR"))
        .map(PathBuf::from)
}

fn link_kind() -> String {
    match env::var("ARCADIA_TIO_CAPI_LINK_KIND") {
        Ok(value) if value == "dylib" || value == "static" => value,
        Ok(other) => {
            panic!("ARCADIA_TIO_CAPI_LINK_KIND must be 'dylib' or 'static', got {other:?}")
        }
        Err(_) => "dylib".to_string(),
    }
}

fn validate_dir(label: &str, path: &Path) {
    if !path.is_dir() {
        panic!(
            "ARCADIA_TIO_CAPI {label} directory does not exist or is not a directory: {}",
            path.display()
        );
    }
}

fn emit_link(lib_dir: &Path, target: &str, link_kind: &str) {
    emit_native_rerun_tracking(lib_dir, target, link_kind);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib={}={}", link_kind, LINK_NAME);
    println!("cargo:metadata=lib_dir={}", lib_dir.display());
    println!("cargo:metadata=link_kind={link_kind}");
    if let Some(lib_file) = expected_library_file(lib_dir, target, link_kind) {
        println!("cargo:metadata=lib_file={}", lib_file.display());
        println!(
            "cargo:rustc-env=ARCADIA_TIO_CAPI_RESOLVED_LIB_FILE={}",
            lib_file.display()
        );
    }
    println!(
        "cargo:rustc-env=ARCADIA_TIO_CAPI_RESOLVED_LIB_DIR={}",
        lib_dir.display()
    );
}

fn emit_include_dir() {
    if let Some(include_dir) = env::var_os("ARCADIA_TIO_CAPI_INCLUDE_DIR").map(PathBuf::from) {
        validate_dir("include", &include_dir);
        emit_resolved_include_dir(&include_dir);
    }
}

fn emit_resolved_include_dir(include_dir: &Path) {
    println!("cargo:rerun-if-changed={}", include_dir.display());
    println!("cargo:metadata=include_dir={}", include_dir.display());
    println!(
        "cargo:rustc-env=ARCADIA_TIO_CAPI_RESOLVED_INCLUDE_DIR={}",
        include_dir.display()
    );
}

fn emit_native_rerun_tracking(lib_dir: &Path, target: &str, link_kind: &str) {
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    for name in expected_library_file_names(target, link_kind) {
        println!("cargo:rerun-if-changed={}", lib_dir.join(name).display());
    }

    if let Some(target_dir) = lib_dir.parent() {
        for manifest_name in [
            "manifest.json",
            "native-manifest.json",
            "arcadia-tio-native.json",
            ".arcadia-tio-native.json",
        ] {
            println!(
                "cargo:rerun-if-changed={}",
                target_dir.join(manifest_name).display()
            );
        }
    }
}

fn env_truthy(key: &str) -> bool {
    matches!(
        env::var(key).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn warn_if_expected_library_missing(lib_dir: &Path, target: &str, link_kind: &str) {
    let candidates = expected_library_file_names(target, link_kind);
    if expected_library_file_from_candidates(lib_dir, &candidates).is_none() {
        println!(
            "cargo:warning=arcadia-tio-sys did not find expected {link_kind} library names [{}] in {}; the platform linker may still resolve {LINK_NAME}",
            candidates.join(", "),
            lib_dir.display()
        );
    }
}

fn expected_library_file(lib_dir: &Path, target: &str, link_kind: &str) -> Option<PathBuf> {
    expected_library_file_from_candidates(lib_dir, &expected_library_file_names(target, link_kind))
}

fn expected_library_file_from_candidates(lib_dir: &Path, candidates: &[String]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|name| lib_dir.join(name))
        .find(|path| path.is_file())
}

fn expected_library_file_names(target: &str, link_kind: &str) -> Vec<String> {
    if link_kind == "static" {
        if target.contains("windows") {
            return vec![
                "arcadia_tio_capi.lib".to_string(),
                "libarcadia_tio_capi.a".to_string(),
            ];
        }
        return vec!["libarcadia_tio_capi.a".to_string()];
    }

    if target.contains("windows") {
        vec![
            "arcadia_tio_capi.dll".to_string(),
            "arcadia_tio_capi.lib".to_string(),
        ]
    } else if target.contains("apple") || target.contains("darwin") {
        vec!["libarcadia_tio_capi.dylib".to_string()]
    } else {
        vec!["libarcadia_tio_capi.so".to_string()]
    }
}
