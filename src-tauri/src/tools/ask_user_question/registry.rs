//! Tauri-backed `UserQuestionAsker`. The flow:
//!
//! 1. Tool execute() calls `TauriUserQuestionAsker::ask`, which:
//!    * generates a request id,
//!    * registers a oneshot sender against the id in the
//!      [`UserQuestionRegistry`] (Tauri-managed state),
//!    * emits a `user-question-pending` event to the webview.
//! 2. Frontend renders a question prompt with the registered options and
//!    asks the user. When the user replies, it invokes the
//!    [`submit_user_answer`] Tauri command.
//! 3. The command looks up the sender by id and forwards the reply.
//! 4. The original tool call resumes with the user's reply as the result.
//!
//! Cancellation, timeouts, and double-replies are all defended at the
//! registry layer.

use super::{UserQuestionAsker, UserQuestionRequest};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Emitter, EventTarget, Manager};
use tokio::sync::oneshot;

/// Reply payload delivered by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    pub request_id: String,
    pub answer: String,
}

/// Event payload emitted to the frontend when a question is pending.
#[derive(Debug, Clone, Serialize)]
pub struct PendingQuestion<'a> {
    pub request_id: &'a str,
    pub agent_id: &'a str,
    pub question: &'a str,
    pub options: &'a [String],
}

/// Tauri-managed state. One instance per app, holds in-flight question
/// senders keyed by request id.
///
/// **Concurrency contract:** `pending` is a `std::sync::Mutex`. Every
/// access in this file completes synchronously (insert / remove / send)
/// — no `.await` is reached while the lock is held. Future maintainers
/// MUST preserve this invariant: holding a std mutex across an
/// `.await` blocks the tokio worker until the awaited future resolves
/// and risks deadlock under contention. If you need to call something
/// async while logically holding the lock, swap to
/// `tokio::sync::Mutex` first.
#[derive(Default)]
pub struct UserQuestionRegistry {
    pending: Mutex<HashMap<String, oneshot::Sender<String>>>,
}

impl UserQuestionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending question and return its receiver.
    fn register(&self, id: String) -> oneshot::Receiver<String> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        rx
    }

    /// Resolve a pending question. Idempotent: a second call with the same
    /// id returns `Err` (already resolved or unknown).
    pub fn resolve(&self, id: &str, answer: String) -> Result<(), String> {
        let tx = {
            let mut pending = self.pending.lock().unwrap();
            pending
                .remove(id)
                .ok_or_else(|| format!("no pending question with id `{id}`"))?
        };
        tx.send(answer)
            .map_err(|_| "receiver dropped before answer arrived".to_string())
    }

    /// Drop a pending question (used on timeout / cancellation) without
    /// resolving it. The receiver will see a closed channel.
    fn drop_pending(&self, id: &str) {
        self.pending.lock().unwrap().remove(id);
    }
}

/// Production asker — emits a Tauri event and awaits the reply via the
/// shared registry.
pub struct TauriUserQuestionAsker;

#[async_trait]
impl UserQuestionAsker for TauriUserQuestionAsker {
    async fn ask(&self, request: UserQuestionRequest<'_>) -> Result<String, String> {
        let registry = request
            .app_handle
            .try_state::<UserQuestionRegistry>()
            .ok_or_else(|| "UserQuestionRegistry not registered on AppHandle".to_string())?;

        let request_id = format!(
            "{}-{}",
            request.agent_id,
            chrono::Utc::now().timestamp_millis()
        );
        let rx = registry.register(request_id.clone());

        let payload = PendingQuestion {
            request_id: &request_id,
            agent_id: request.agent_id,
            question: request.question,
            options: request.options,
        };

        if let Err(e) = request.app_handle.emit_to(
            EventTarget::webview_window(request.window_label),
            "user-question-pending",
            payload,
        ) {
            // Emit failed — most likely the target webview is gone.
            // We've already registered a sender; drop it now so the
            // entry doesn't leak (registry would otherwise hold a stale
            // pending question for the lifetime of the app).
            registry.drop_pending(&request_id);
            return Err(format!("emit user-question-pending: {e}"));
        }

        // Await the user's reply with timeout + cancellation, dropping the
        // pending entry on every exit path so a stale id can't accept a
        // late reply.
        let result = tokio::select! {
            biased;
            _ = request.token.cancelled() => Err("cancelled".to_string()),
            answered = tokio::time::timeout(request.timeout, rx) => {
                match answered {
                    Ok(Ok(reply)) => Ok(reply),
                    Ok(Err(_)) => Err("question channel closed".to_string()),
                    Err(_) => Err(format!(
                        "user did not reply within {}s",
                        request.timeout.as_secs()
                    )),
                }
            }
        };

        if result.is_err() {
            registry.drop_pending(&request_id);
            // Best-effort UI cleanup so the prompt doesn't linger after
            // a timeout / cancellation.
            let _ = request.app_handle.emit_to(
                EventTarget::webview_window(request.window_label),
                "user-question-cancelled",
                serde_json::json!({ "request_id": request_id }),
            );
        }

        result
    }
}

/// Tauri command — frontend invokes this to deliver a user's reply.
#[tauri::command]
pub fn submit_user_answer(
    state: tauri::State<'_, UserQuestionRegistry>,
    payload: UserAnswer,
) -> Result<(), String> {
    state.resolve(&payload.request_id, payload.answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_unknown_id_returns_error() {
        let reg = UserQuestionRegistry::new();
        let err = reg.resolve("nope", "hi".to_string()).unwrap_err();
        assert!(err.contains("no pending"));
    }

    #[tokio::test]
    async fn register_then_resolve_delivers_answer() {
        let reg = UserQuestionRegistry::new();
        let rx = reg.register("q1".to_string());
        reg.resolve("q1", "yes".to_string()).unwrap();
        let answer = rx.await.unwrap();
        assert_eq!(answer, "yes");
    }

    #[tokio::test]
    async fn second_resolve_for_same_id_errors() {
        let reg = UserQuestionRegistry::new();
        let _rx = reg.register("q2".to_string());
        reg.resolve("q2", "first".to_string()).unwrap();
        let err = reg.resolve("q2", "second".to_string()).unwrap_err();
        assert!(err.contains("no pending"));
    }

    #[tokio::test]
    async fn drop_pending_closes_channel() {
        let reg = UserQuestionRegistry::new();
        let rx = reg.register("q3".to_string());
        reg.drop_pending("q3");
        // Receiver sees a closed channel.
        assert!(rx.await.is_err());
        // Resolve after drop is a no-op error.
        assert!(reg.resolve("q3", "late".to_string()).is_err());
    }
}
