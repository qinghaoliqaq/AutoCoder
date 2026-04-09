/// Build gate — compile/test validation between implementation and review.
///
/// Auto-detects the project build system from the workspace (Cargo.toml,
/// package.json, tsconfig.json) and runs appropriate check commands.
/// Returns structured results so compile errors can be fed back to Claude.
use std::path::Path;
use std::process::Output;
use tokio::process::Command;

const MAX_OUTPUT_CHARS: usize = 6000;
const BUILD_TIMEOUT_SECS: u64 = 180;

// ── Data types ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub(crate) struct BuildCommand {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct BuildGateResult {
    pub passed: bool,
    pub results: Vec<CommandResult>,
}

#[derive(Clone, Debug)]
pub(crate) struct CommandResult {
    pub label: String,
    pub command: String,
    pub passed: bool,
    /// Combined stdout+stderr, truncated to keep prompt size reasonable.
    pub output: String,
}

impl BuildGateResult {
    /// Render a concise summary suitable for injection into a fix prompt.
    pub fn failure_summary(&self) -> String {
        let mut out = String::new();
        for result in &self.results {
            if !result.passed {
                out.push_str(&format!("### {} (`{}`)\n", result.label, result.command));
                out.push_str(&result.output);
                out.push('\n');
            }
        }
        out
    }
}

// ── Detection ────────────────────────────────────────────────────────────

/// Auto-detect build commands from the workspace contents.
/// Returns an empty vec if no known build system is found.
pub(crate) fn detect_build_commands(workspace: &Path) -> Vec<BuildCommand> {
    let mut commands = Vec::new();

    // Rust project — fast type/borrow check without full codegen.
    if workspace.join("Cargo.toml").exists() {
        commands.push(BuildCommand {
            label: "Rust compile check".to_string(),
            program: "cargo".to_string(),
            args: vec!["check".to_string(), "--message-format=short".to_string()],
        });
    }

    // Node / TypeScript project.
    if workspace.join("package.json").exists() {
        if let Some(ts_cmd) = detect_typescript_check(workspace) {
            commands.push(ts_cmd);
        }
    }

    commands
}

fn detect_typescript_check(workspace: &Path) -> Option<BuildCommand> {
    // Prefer a `typecheck` npm script if defined — projects often customize it.
    if let Ok(pkg_json) = std::fs::read_to_string(workspace.join("package.json")) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&pkg_json) {
            if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
                if scripts.contains_key("typecheck") {
                    return Some(BuildCommand {
                        label: "TypeScript type check (npm)".to_string(),
                        program: npm_cmd(),
                        args: vec!["run".to_string(), "typecheck".to_string()],
                    });
                }
            }
        }
    }

    // Fall back to raw tsc --noEmit if tsconfig.json exists.
    if workspace.join("tsconfig.json").exists() {
        return Some(BuildCommand {
            label: "TypeScript type check (tsc)".to_string(),
            program: npx_cmd(),
            args: vec!["tsc".to_string(), "--noEmit".to_string()],
        });
    }

    None
}

fn npm_cmd() -> String {
    if cfg!(windows) {
        "npm.cmd".to_string()
    } else {
        "npm".to_string()
    }
}

fn npx_cmd() -> String {
    if cfg!(windows) {
        "npx.cmd".to_string()
    } else {
        "npx".to_string()
    }
}

// ── Execution ────────────────────────────────────────────────────────────

/// Run all detected build commands sequentially, stopping on first failure.
/// If Node dependencies are missing (node_modules absent), installs them first.
pub(crate) async fn run_build_gate(workspace: &Path, commands: &[BuildCommand]) -> BuildGateResult {
    // Ensure Node dependencies are available before running TS/JS checks.
    // The isolated workspace is created by copying the main workspace but
    // skipping node_modules (too large).  If Claude didn't run `npm install`
    // during implementation, every Node-based check would fail.
    let needs_node = commands
        .iter()
        .any(|c| c.program.starts_with("npm") || c.program.starts_with("npx"));
    if needs_node && !workspace.join("node_modules").exists() {
        if let Some(install_result) = ensure_node_deps(workspace).await {
            if !install_result.passed {
                return BuildGateResult {
                    passed: false,
                    results: vec![install_result],
                };
            }
        }
    }

    let mut results = Vec::new();
    let mut all_passed = true;

    for cmd in commands {
        let result = run_command(workspace, cmd).await;
        if !result.passed {
            all_passed = false;
        }
        results.push(result);

        // Stop on first failure — no point running later checks if build fails.
        if !all_passed {
            break;
        }
    }

    BuildGateResult {
        passed: all_passed,
        results,
    }
}

/// Detect the package manager and install dependencies.
/// Returns None if no lock file is found (nothing to install).
///
/// Uses non-strict install mode (`npm install` instead of `npm ci`,
/// no `--frozen-lockfile`) because Claude's implementation step may
/// have added new dependencies to package.json that aren't yet in
/// the lock file.  Strict modes would fail every time in that case.
async fn ensure_node_deps(workspace: &Path) -> Option<CommandResult> {
    let (program, args, label) = if workspace.join("pnpm-lock.yaml").exists() {
        ("pnpm", vec!["install"], "pnpm install")
    } else if workspace.join("yarn.lock").exists() {
        ("yarn", vec!["install"], "yarn install")
    } else if workspace.join("bun.lockb").exists()
        || workspace.join("bun.lock").exists()
    {
        ("bun", vec!["install"], "bun install")
    } else {
        ("npm", vec!["install"], "npm install")
    };

    tracing::info!(workspace = %workspace.display(), label, "Installing Node dependencies for build gate");

    let install_cmd = BuildCommand {
        label: label.to_string(),
        program: program.to_string(),
        args: args.into_iter().map(|s| s.to_string()).collect(),
    };
    Some(run_command(workspace, &install_cmd).await)
}

async fn run_command(workspace: &Path, cmd: &BuildCommand) -> CommandResult {
    let command_str = format!("{} {}", cmd.program, cmd.args.join(" "));

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(BUILD_TIMEOUT_SECS),
        Command::new(&cmd.program)
            .args(&cmd.args)
            .current_dir(workspace)
            .output(),
    )
    .await;

    match output {
        Ok(Ok(output)) => {
            let passed = output.status.success();
            let combined = format_output(&output);
            CommandResult {
                label: cmd.label.clone(),
                command: command_str,
                passed,
                output: truncate_output(&combined),
            }
        }
        Ok(Err(err)) => CommandResult {
            label: cmd.label.clone(),
            command: command_str,
            passed: false,
            output: format!("Failed to execute command: {err}"),
        },
        Err(_) => CommandResult {
            label: cmd.label.clone(),
            command: command_str,
            passed: false,
            output: format!("Command timed out after {BUILD_TIMEOUT_SECS}s"),
        },
    }
}

fn format_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    // Stderr first — compiler errors are typically here.
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !stdout.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stdout);
    }
    combined
}

/// Keep the tail of the output (most actionable errors are usually at the end).
fn truncate_output(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    let start = text.len() - MAX_OUTPUT_CHARS;
    // Advance to next newline to avoid cutting mid-line.
    let start = text[start..]
        .find('\n')
        .map(|i| start + i + 1)
        .unwrap_or(start);
    format!("[... truncated ...]\n{}", &text[start..])
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let commands = detect_build_commands(dir.path());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].label, "Rust compile check");
        assert!(commands[0].args.contains(&"check".to_string()));
    }

    #[test]
    fn detect_typescript_project_with_tsconfig() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let commands = detect_build_commands(dir.path());
        assert_eq!(commands.len(), 1);
        assert!(commands[0].label.contains("TypeScript"));
    }

    #[test]
    fn detect_typescript_prefers_npm_typecheck_script() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"test","scripts":{"typecheck":"tsc --noEmit"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let commands = detect_build_commands(dir.path());
        assert_eq!(commands.len(), 1);
        assert!(commands[0].label.contains("npm"));
    }

    #[test]
    fn detect_mixed_rust_and_typescript() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let commands = detect_build_commands(dir.path());
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn detect_no_build_system() {
        let dir = tempfile::tempdir().unwrap();
        let commands = detect_build_commands(dir.path());
        assert!(commands.is_empty());
    }

    #[test]
    fn truncate_output_preserves_short_text() {
        let text = "hello world";
        assert_eq!(truncate_output(text), text);
    }

    #[test]
    fn truncate_output_keeps_tail() {
        let line = "x".repeat(100);
        let text = format!("{line}\n{line}\n{line}");
        let truncated = truncate_output(&text);
        // The original is 302 chars, well under MAX_OUTPUT_CHARS, so no truncation.
        assert_eq!(truncated, text);
    }

    #[test]
    fn failure_summary_only_includes_failed_commands() {
        let result = BuildGateResult {
            passed: false,
            results: vec![
                CommandResult {
                    label: "Rust check".to_string(),
                    command: "cargo check".to_string(),
                    passed: true,
                    output: "ok".to_string(),
                },
                CommandResult {
                    label: "TS check".to_string(),
                    command: "tsc --noEmit".to_string(),
                    passed: false,
                    output: "error TS2345: type mismatch".to_string(),
                },
            ],
        };
        let summary = result.failure_summary();
        assert!(!summary.contains("Rust check"));
        assert!(summary.contains("TS check"));
        assert!(summary.contains("error TS2345"));
    }

    #[tokio::test]
    async fn run_build_gate_stops_on_first_failure() {
        // Use a command that always fails and one that would succeed.
        let commands = vec![
            BuildCommand {
                label: "Always fails".to_string(),
                program: "false".to_string(),
                args: Vec::new(),
            },
            BuildCommand {
                label: "Would succeed".to_string(),
                program: "true".to_string(),
                args: Vec::new(),
            },
        ];
        let dir = tempfile::tempdir().unwrap();
        let result = run_build_gate(dir.path(), &commands).await;
        assert!(!result.passed);
        // Only the first command should have been executed.
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].label, "Always fails");
    }

    #[tokio::test]
    async fn run_build_gate_passes_when_all_succeed() {
        let commands = vec![
            BuildCommand {
                label: "Step 1".to_string(),
                program: "true".to_string(),
                args: Vec::new(),
            },
            BuildCommand {
                label: "Step 2".to_string(),
                program: "true".to_string(),
                args: Vec::new(),
            },
        ];
        let dir = tempfile::tempdir().unwrap();
        let result = run_build_gate(dir.path(), &commands).await;
        assert!(result.passed);
        assert_eq!(result.results.len(), 2);
    }
}
