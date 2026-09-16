// tests/architecture_test.rs — enforcement of the code-structure / dependency
// rule (code-structure.md, ports-and-adapters.md NOGOs).
//
// The slice's CORE files (`model.rs`, `<slice>.rs`, `*_repository.rs`,
// `error.rs`) must not import `axum` / `sqlx` / `serde` / `tokio` or the
// slice's own adapters (`pg_settings.rs`, `rest.rs`). There is no layer-named
// top-level folder (`domain/`, `infrastructure/`, `interfaces/`, `application/`).

use std::fs;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Return the module-level (non-`#[cfg(test)]`) text of a source file: strip
/// everything after the first `#[cfg(test)]` so test-only imports (real sqlx /
/// axum usage in integration tests) don't trip the domain-dependency rule.
fn module_level_text(path: &PathBuf) -> String {
    let src = fs::read_to_string(path).expect("read source");
    match src.find("#[cfg(test)]") {
        Some(idx) => src[..idx].to_string(),
        None => src,
    }
}

const FORBIDDEN_IMPORTS: [&str; 8] = [
    "use axum::",
    "use sqlx::",
    "use serde::",
    "use tokio::",
    "use crate::settings::pg_settings::",
    "use crate::settings::rest::",
    "use crate::auth::rest::",
    "use crate::auth::jwt::",
];

const CORE_FILES: [&str; 4] = [
    "model.rs",
    "settings.rs",
    "settings_repository.rs",
    "error.rs",
];

const LAYER_DIRS: [&str; 4] = ["domain", "infrastructure", "interfaces", "application"];

// be-arch §3 / §6: the domain & service core must never import an *adapter* or
// a *transport* (HTTP/serialization) type from its own slice, and cross-context
// access is only via the shared kernel (the `auth` model's Authority/Ownership).
// These forbid the auth slice's adapters leaking into the settings core and the
// settings slice's adapters leaking into the auth core. The final blanket rule
// (``use crate::settings::``) applies ONLY to the `auth` slice — `auth` must
// never depend on the settings slice at all; settings may import `auth`'s
// kernel types, so it is exempt from that last check.
const CROSS_ADAPTER_IMPORTS: [&str; 6] = [
    "use crate::settings::pg_settings::",
    "use crate::settings::rest::",
    "use crate::settings::settings::",
    "use crate::auth::rest::",
    "use crate::auth::jwt::",
    "use crate::auth::revocation::",
];

// `auth` slice must never reach into `settings` (slices communicate through
// ports / injected values, never a direct module import).
const AUTH_NO_SETTINGS: &str = "use crate::settings::";

#[test]
fn no_adapter_or_transport_types_in_any_slice_core() {
    for slice in ["settings", "auth"] {
        for file in CORE_FILES {
            let path = crate_root().join("src").join(slice).join(file);
            if !path.exists() {
                continue;
            }
            let text = module_level_text(&path);
            for forbidden in CROSS_ADAPTER_IMPORTS {
                assert!(
                    !text.contains(forbidden),
                    "{slice}/{file} must not import `{forbidden}`"
                );
            }
        }
    }
    // Cross-context: the auth slice must not import any settings module.
    for file in CORE_FILES {
        let path = crate_root().join("src").join("auth").join(file);
        if !path.exists() {
            continue;
        }
        let text = module_level_text(&path);
        assert!(
            !text.contains(AUTH_NO_SETTINGS),
            "auth/{file} must not import `{AUTH_NO_SETTINGS}`"
        );
    }
}

#[test]
fn no_layer_named_top_level_dirs() {
    let src = crate_root().join("src");
    for entry in fs::read_dir(&src).expect("read src") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                !LAYER_DIRS.contains(&name.as_str()),
                "layer-named top-level dir found: {name}"
            );
        }
    }
}

#[test]
fn slice_core_files_do_not_import_frameworks_or_adapters() {
    for slice in ["settings", "auth"] {
        for file in CORE_FILES {
            let path = crate_root().join("src").join(slice).join(file);
            if !path.exists() {
                continue;
            }
            let text = module_level_text(&path);
            for forbidden in FORBIDDEN_IMPORTS {
                assert!(
                    !text.contains(forbidden),
                    "{slice}/{file} must not import `{forbidden}`"
                );
            }
        }
    }
}

#[test]
fn no_domain_sub_folder_inside_a_slice_and_no_flat_violation() {
    for slice in ["settings", "auth"] {
        let dir = crate_root().join("src").join(slice);
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir).expect("read slice") {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                !matches!(name.as_str(), "domain" | "infra" | "dto"),
                "slice {slice} must stay flat; found sub-folder {name}"
            );
        }
    }
}
