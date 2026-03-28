/// Debug skill — Codex investigates and fixes the reported issue.

use crate::prompts::Prompts;
use super::runners;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task:         &str,
    workspace:    Option<&str>,
    context:      Option<&str>,
    prompts:      &Prompts,
    window_label: &str,
    app_handle:   &tauri::AppHandle,
    token:        CancellationToken,
) -> Result<(), String> {
    let prompt = super::inject_context(
        context,
        Prompts::render(&prompts.debug_codex, &[("issue", task)]),
    );
    runners::codex(&prompt, workspace, window_label, app_handle, token).await?;
    Ok(())
}
