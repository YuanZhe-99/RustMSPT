# `version` — what this binary is

## What it does

Reports the build: package version, git commit, whether the worktree was dirty when the build script
last ran, the enabled cargo features, and the build platform. `rustmspt --version` prints one line
and `rustmspt version --json` prints the same facts as a JSON object.

It exists because a program that drives this one as an external tool has to record which bytes
produced every artifact. Before it existed, the only way to answer that was to hash the binary and
read the checkout's `HEAD` from outside.

## Running it

```bash
./target/release/rustmspt --version
```

```bash
./target/release/rustmspt version --json
```

## Expected output

Real captured output from a release build:

```
rustmspt 0.2.0 (git 77642fd, dirty; features: default)
```

```json
{
  "name": "rustmspt",
  "version": "0.2.0",
  "git_commit": "77642fdae99098cf810984f3b5086d469a721e8e",
  "git_dirty": true,
  "features": [
    "default"
  ],
  "target": "aarch64-unknown-linux-gnu",
  "host": "aarch64-unknown-linux-gnu",
  "profile": "release",
  "source_date_epoch": null
}
```

`--version` and `version` print identical text: both are `build_identity().version_line()`.

The commit and the dirty flag above are from the particular build that produced this capture. Your
own build will report its own, which is the whole point of the subcommand; do not read these two
values as constants.

## What the fields mean, and what they do not claim

| Field | Meaning |
|---|---|
| `version` | The package version from `Cargo.toml`, read at compile time. |
| `git_commit` | The full 40-character commit the crate was built from, or `null`. |
| `git_dirty` | Whether the worktree had uncommitted changes when `build.rs` last ran. |
| `features` | Enabled cargo features, sorted. `default` appears because the crate declares one. |
| `target` / `host` | The compilation target and the machine that compiled it. |
| `profile` | `debug` or `release`. |
| `source_date_epoch` | `SOURCE_DATE_EPOCH`, when set for a reproducible build. |

Two properties are deliberate.

**A checkout without git still builds, and still answers.** Every git call is wrapped so that a
missing git binary, an absent `.git`, or a repository with no commits all yield `null` rather than a
failure. Nothing panics and nothing emits a build warning.

**`null` is not `false`.** "We could not determine whether the worktree was dirty" and "the worktree
was clean" are different claims, so `git_dirty` is nullable, and cleanliness is only reported when a
commit can also be named. A consumer recording this into an artifact needs to be able to tell the
two apart.

The honesty limit is worth stating: `git_dirty` describes the worktree at the moment `build.rs` last
ran, which can predate an edit made after the last compile. The build script reruns whenever
`build.rs`, `Cargo.toml`, `Cargo.lock`, `src/` or the git refs change, which narrows that window but
cannot close it.

## Notes

- `--help` is unchanged in shape: its first line is still the program's description and `Usage:` is
  still the third line. Adding `version` put one more row in the subcommand list and nothing else.
- The same identity object is embedded as `tool` in the placement record and run report, so an
  artifact carries the build that produced it.
- See [`../reference/core-and-compute.md`](../reference/core-and-compute.md) for `BuildIdentity`,
  `build_identity`, and the `build.rs` functions that stamp the values in.
