use rustmspt::version::{build_identity, identity_json};

// The identity must name the package version the crate was compiled with, not a
// literal that can drift from Cargo.toml.
#[test]
fn identity_reports_the_compiled_package_version() {
    let identity = build_identity();
    assert_eq!(identity.name, "rustmspt");
    assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
}

// A build with no git, no .git, or no commit must still produce an identity;
// the undetermined fields are None, never a fabricated value.
#[test]
fn undetermined_fields_are_none_not_fabricated() {
    let identity = build_identity();
    if let Some(commit) = identity.git_commit {
        assert_eq!(commit.len(), 40, "a commit is a full 40-hex sha1: {commit}");
        assert!(
            commit.chars().all(|c| c.is_ascii_hexdigit()),
            "a commit is hexadecimal: {commit}"
        );
    }
    // git_dirty is Option<bool>: unknown is a third state, distinct from clean.
    assert!(matches!(identity.git_dirty, Some(true) | Some(false) | None));
    // Cleanliness is only meaningful once a commit is named.
    if identity.git_commit.is_none() {
        assert!(identity.git_dirty.is_none());
    }
}

// build.rs reads CARGO_FEATURE_*, so the gpu feature must appear exactly when
// the crate was built with it. Reading cfg!(feature) inside build.rs would not
// work: a build script compiles without the crate's features.
#[test]
fn features_track_the_build() {
    let identity = build_identity();
    let has_gpu = identity.features.contains(&"gpu");
    assert_eq!(has_gpu, cfg!(feature = "gpu"));
    let mut sorted = identity.features.clone();
    sorted.sort();
    assert_eq!(sorted, identity.features, "features are reported sorted");
    assert!(
        !identity.features.iter().any(|f| f.is_empty()),
        "no empty feature names"
    );
}

// The JSON is what SimHelper records in every artifact, so every key must be
// present, with null (not a placeholder) for undetermined values.
#[test]
fn identity_json_has_every_key() {
    let identity = build_identity();
    let json = identity_json(&identity);
    let value: serde_json::Value = serde_json::from_str(&json).expect("identity JSON parses");
    let object = value.as_object().expect("identity JSON is an object");
    for key in [
        "name",
        "version",
        "git_commit",
        "git_dirty",
        "features",
        "target",
        "host",
        "profile",
        "source_date_epoch",
    ] {
        assert!(object.contains_key(key), "identity JSON has key {key}");
    }
    assert_eq!(object["name"], "rustmspt");
    assert_eq!(object["version"], env!("CARGO_PKG_VERSION"));
    assert!(object["features"].is_array());
    assert!(object["git_dirty"].is_boolean() || object["git_dirty"].is_null());
    assert!(object["git_commit"].is_string() || object["git_commit"].is_null());
}

// clap prepends the program name to its --version string, so version_detail
// must omit it and version_line must add it back exactly once.
#[test]
fn version_line_is_the_name_plus_the_detail() {
    let identity = build_identity();
    let detail = identity.version_detail();
    let line = identity.version_line();
    assert!(!detail.starts_with("rustmspt"), "detail omits the name: {detail}");
    assert_eq!(line, format!("rustmspt {detail}"));
    assert!(line.contains(env!("CARGO_PKG_VERSION")));
    assert_eq!(line.matches("rustmspt").count(), 1, "the name appears once");
}
