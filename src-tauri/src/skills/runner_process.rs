/// Process lifecycle management for CLI runners (Claude Code / Codex).
///
/// Provides a PID registry, RAII guard for automatic cleanup, and
/// platform-specific termination helpers.  All skill modules go through
/// `runners` which delegates to this module — nothing else spawns processes.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::process::Command;

// ── PID registry ──────────────────────────────────────────────────────────

static RUNNER_PIDS: OnceLock<Mutex<HashMap<String, Vec<u32>>>> = OnceLock::new();

fn runner_pid_registry() -> &'static Mutex<HashMap<String, Vec<u32>>> {
    RUNNER_PIDS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn register_runner_pid(window_label: &str, pid: u32) {
    let mut registry = runner_pid_registry().lock().unwrap();
    let entry = registry.entry(window_label.to_string()).or_default();
    if !entry.contains(&pid) {
        entry.push(pid);
    }
}

pub(super) fn unregister_runner_pid(window_label: &str, pid: u32) {
    let mut registry = runner_pid_registry().lock().unwrap();
    if let Some(entry) = registry.get_mut(window_label) {
        entry.retain(|registered| *registered != pid);
        if entry.is_empty() {
            registry.remove(window_label);
        }
    }
}

pub(crate) fn kill_registered_processes(window_label: &str) {
    let pids = {
        let registry = runner_pid_registry().lock().unwrap();
        registry.get(window_label).cloned().unwrap_or_default()
    };

    for pid in pids {
        terminate_process(pid);
    }
}

// ── Termination ───────────────────────────────────────────────────────────

fn terminate_process(pid: u32) {
    #[cfg(unix)]
    {
        let pid_str = pid.to_string();
        let process_group = format!("-{pid}");
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &process_group])
            .status();
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid_str])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(60));
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &process_group])
            .status();
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &pid_str])
            .status();
    }

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
}

pub(super) fn isolate_child_process_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
}

// ── RAII guard ────────────────────────────────────────────────────────────

pub(super) struct ChildProcessGuard {
    window_label: String,
    pid: Option<u32>,
}

impl ChildProcessGuard {
    pub fn new(window_label: &str, pid: Option<u32>) -> Self {
        if let Some(pid) = pid {
            register_runner_pid(window_label, pid);
        }
        Self {
            window_label: window_label.to_string(),
            pid,
        }
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            unregister_runner_pid(&self.window_label, pid);
        }
    }
}
