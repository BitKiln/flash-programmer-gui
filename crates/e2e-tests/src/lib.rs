//! Harness for the opaque-box end-to-end suite.
//!
//! Every test in this crate drives the real `flashgui-cli` binary as a child
//! process. Nothing here reaches into the library crates: if a scenario passes,
//! it passes because the shipped executable behaved correctly.
//!
//! Two pieces of global state would otherwise make these tests interfere with
//! each other, so [`Cli`] isolates both per invocation:
//!
//! * the mock backend persists simulated flash contents in a file under the
//!   system temp directory, so each scenario gets its own `TMP`/`TEMP`;
//! * profiles resolve against `./.flashgui/profiles`, so each scenario runs in
//!   its own working directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use tempfile::TempDir;

/// Repository root, derived from this crate's manifest location.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives at <root>/crates/e2e-tests")
        .to_path_buf()
}

/// Absolute path of a file under `tests/fixtures/`.
pub fn fixture(name: &str) -> PathBuf {
    let path = repo_root().join("tests").join("fixtures").join(name);
    assert!(path.is_file(), "missing fixture: {}", path.display());
    path
}

/// Path to the `flashgui-cli` executable, building it on first use.
///
/// `cargo test -p e2e-tests` does not build the binaries of path dependencies,
/// so the harness builds it explicitly rather than silently testing a stale or
/// absent executable.
pub fn cli_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let status = Command::new(env!("CARGO"))
            .current_dir(repo_root())
            .args(["build", "-p", "flashgui-cli", "--bin", "flashgui-cli"])
            .status()
            .expect("failed to run cargo build for flashgui-cli");
        assert!(status.success(), "cargo build of flashgui-cli failed");

        // <target>/<profile>/deps/<test exe> -> <target>/<profile>/flashgui-cli
        let test_exe = std::env::current_exe().expect("test executable path");
        let profile_dir = test_exe
            .parent()
            .and_then(Path::parent)
            .expect("test exe lives in <profile>/deps")
            .to_path_buf();
        let binary = profile_dir.join(format!("flashgui-cli{}", std::env::consts::EXE_SUFFIX));
        assert!(
            binary.is_file(),
            "flashgui-cli not found at {}",
            binary.display()
        );
        binary
    })
}

/// Outcome of one CLI invocation.
#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Asserts the process exit code, reporting both streams on failure.
    #[track_caller]
    pub fn assert_code(&self, expected: i32) -> &Self {
        assert_eq!(
            self.code, expected,
            "expected exit {expected}, got {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.code, self.stdout, self.stderr
        );
        self
    }

    #[track_caller]
    pub fn assert_stdout_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stdout.contains(needle),
            "stdout did not contain {needle:?}\n--- stdout ---\n{}",
            self.stdout
        );
        self
    }

    /// Parses stdout as NDJSON, one JSON value per non-empty line.
    #[track_caller]
    pub fn ndjson(&self) -> Vec<serde_json::Value> {
        self.stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"))
            })
            .collect()
    }

    /// All NDJSON messages whose `type` field equals `kind`.
    pub fn ndjson_of_type(&self, kind: &str) -> Vec<serde_json::Value> {
        self.ndjson()
            .into_iter()
            .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some(kind))
            .collect()
    }
}

/// An isolated CLI sandbox: its own temp directory (mock flash state) and its
/// own working directory (profiles). Mock flash therefore persists across
/// invocations made through the same `Cli`, and only those.
pub struct Cli {
    home: TempDir,
}

impl Cli {
    pub fn new() -> Self {
        Self {
            home: TempDir::new().expect("failed to create sandbox directory"),
        }
    }

    /// The sandbox working directory, where `.flashgui/profiles` will appear.
    pub fn dir(&self) -> &Path {
        self.home.path()
    }

    /// Runs the CLI with the given arguments inside this sandbox.
    pub fn run(&self, args: &[&str]) -> Output {
        let out = Command::new(cli_binary())
            .current_dir(self.home.path())
            .env("TMP", self.home.path())
            .env("TEMP", self.home.path())
            .env("TMPDIR", self.home.path())
            .args(args)
            .output()
            .expect("failed to spawn flashgui-cli");

        Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Runs the CLI against the mock backend (`--mock` prepended).
    pub fn mock(&self, args: &[&str]) -> Output {
        let mut full = vec!["--mock"];
        full.extend_from_slice(args);
        self.run(&full)
    }
}

impl Default for Cli {
    fn default() -> Self {
        Self::new()
    }
}

/// Target name used throughout the mock tiers.
pub const MOCK_TARGET: &str = "STM32F401RE";

/// Documented CLI exit-code contract, restated here rather than imported so the
/// tests fail if the binary's codes ever drift from the published contract.
pub mod exit {
    pub const SUCCESS: i32 = 0;
    pub const FLASH_VERIFY_ERROR: i32 = 1;
    pub const TARGET_CONNECTION_ERROR: i32 = 2;
    pub const FIRMWARE_PARSE_ERROR: i32 = 3;
    pub const PROBE_NOT_FOUND: i32 = 4;
    pub const INVALID_ARGS_OR_PROFILE: i32 = 5;
}
