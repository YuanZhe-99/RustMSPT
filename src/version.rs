use serde::Serialize;

// AI-FUNC-SUMMARY: Turn a build-script environment string into None when it is empty; returns Option<&str>; side effects: none.
fn non_empty(value: &'static str) -> Option<&'static str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// What this binary is, as far as the build could determine it.
///
/// Every optional field is `None` when the build could not establish it (no git,
/// no `.git`, or a repository with no commits). `None` is not `false`: "unknown
/// whether the tree was dirty" and "the tree was clean" are different claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildIdentity {
    pub name: &'static str,
    pub version: &'static str,
    pub git_commit: Option<&'static str>,
    pub git_dirty: Option<bool>,
    pub features: Vec<&'static str>,
    pub target: Option<&'static str>,
    pub host: Option<&'static str>,
    pub profile: Option<&'static str>,
    pub source_date_epoch: Option<&'static str>,
}

// AI-FUNC-SUMMARY:
// Purpose: Report the package version, git commit, worktree cleanliness, enabled features and build platform of this binary.
// Inputs: None (compile-time environment stamped by build.rs).
// Returns: BuildIdentity with None wherever the build could not determine a value.
// Side effects: None.
// Notes: The single source of truth for R1; the version subcommand, the placement record and the run report all call it.
pub fn build_identity() -> BuildIdentity {
    let features: Vec<&'static str> = non_empty(env!("RUSTMSPT_FEATURES"))
        .map(|list| list.split(',').filter(|f| !f.is_empty()).collect())
        .unwrap_or_default();
    BuildIdentity {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        git_commit: non_empty(env!("RUSTMSPT_GIT_COMMIT")),
        git_dirty: match env!("RUSTMSPT_GIT_DIRTY") {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        features,
        target: non_empty(env!("RUSTMSPT_BUILD_TARGET")),
        host: non_empty(env!("RUSTMSPT_BUILD_HOST")),
        profile: non_empty(env!("RUSTMSPT_BUILD_PROFILE")),
        source_date_epoch: non_empty(env!("RUSTMSPT_SOURCE_DATE_EPOCH")),
    }
}

impl BuildIdentity {
    // AI-FUNC-SUMMARY:
    // Purpose: Render the identity without the program name, for clap's --version, which prepends the name itself.
    // Inputs: self.
    // Returns: A string such as "0.2.0 (git 0a8eb1c, clean; features: none)".
    // Side effects: None.
    // Notes: An unknown commit prints "git unknown"; unknown cleanliness prints "dirt unknown".
    pub fn version_detail(&self) -> String {
        let commit = match self.git_commit {
            Some(c) if c.len() >= 7 => format!("git {}", &c[..7]),
            Some(c) => format!("git {c}"),
            None => "git unknown".to_string(),
        };
        let dirty = match self.git_dirty {
            Some(true) => "dirty",
            Some(false) => "clean",
            None => "dirt unknown",
        };
        let features = if self.features.is_empty() {
            "none".to_string()
        } else {
            self.features.join(",")
        };
        format!(
            "{} ({}, {}; features: {})",
            self.version, commit, dirty, features
        )
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Render the identity as one human-readable line including the program name.
    // Inputs: self.
    // Returns: A line such as "rustmspt 0.2.0 (git 0a8eb1c, clean; features: none)".
    // Side effects: None.
    // Notes: What `rustmspt version` prints; clap's --version prints the same text because it prepends the name to version_detail.
    pub fn version_line(&self) -> String {
        format!("{} {}", self.name, self.version_detail())
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Serialize a build identity as pretty-printed JSON for `rustmspt version --json`.
// Inputs: the identity.
// Returns: A JSON object with every field present, using null for undetermined values.
// Side effects: None.
// Notes: The same object is embedded as "tool" in the placement record and run report.
pub fn identity_json(identity: &BuildIdentity) -> String {
    serde_json::to_string_pretty(identity).unwrap_or_else(|_| "{}".to_string())
}
