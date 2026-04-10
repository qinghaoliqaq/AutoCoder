/// Typed error hierarchy for the entire application.
///
/// Replaces ad-hoc `Result<T, String>` with discriminated error kinds so that
/// callers can match on the variant (e.g. retry on transient API failures,
/// surface permanent errors to the user immediately).
use serde::Serialize;
use std::fmt;

// ── Error kind ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AppError {
    /// User or system triggered cancellation — not a real failure.
    Cancelled,

    /// Configuration is missing or invalid.
    Config(String),

    /// Network / HTTP layer error (timeouts, DNS, connection refused).
    /// Typically transient and retryable.
    Network(String),

    /// API returned a non-success status code.
    Api { status: u16, body: String },

    /// API returned a response we could not parse.
    ApiParse(String),

    /// Overloaded (429) or server error (5xx) — explicitly retryable.
    ApiOverloaded { status: u16, body: String },

    /// Local tool execution error (bash, editor, grep, glob).
    Tool { tool: String, detail: String },

    /// Filesystem I/O error.
    Io(String),

    /// Git / merge error during parallel subtask execution.
    Merge(String),

    /// Verifier check failed — the implementation doesn't match the plan.
    Verification(String),

    /// Catch-all for errors that don't fit the above categories.
    Internal(String),
}

impl AppError {
    /// Returns `true` when the error is transient and the operation should be
    /// retried with backoff.
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::Network(_) | AppError::ApiOverloaded { .. })
    }

    /// Build an `Api` or `ApiOverloaded` variant depending on the status code.
    pub fn from_api_status(status: u16, body: String) -> Self {
        if status == 429 || status >= 500 {
            AppError::ApiOverloaded { status, body }
        } else {
            AppError::Api { status, body }
        }
    }
}

// ── Display ────────────────────────────────────────────────────────────────

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Cancelled => write!(f, "cancelled"),
            AppError::Config(msg) => write!(f, "config error: {msg}"),
            AppError::Network(msg) => write!(f, "network error: {msg}"),
            AppError::Api { status, body } => write!(f, "API error {status}: {body}"),
            AppError::ApiOverloaded { status, body } => {
                write!(f, "API overloaded {status}: {body}")
            }
            AppError::ApiParse(msg) => write!(f, "API parse error: {msg}"),
            AppError::Tool { tool, detail } => write!(f, "{tool}: {detail}"),
            AppError::Io(msg) => write!(f, "IO error: {msg}"),
            AppError::Merge(msg) => write!(f, "merge error: {msg}"),
            AppError::Verification(msg) => write!(f, "verification failed: {msg}"),
            AppError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

// ── Conversions ────────────────────────────────────────────────────────────

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::ApiParse(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Network(e.to_string())
    }
}

/// Backward-compat: convert AppError → String for Tauri commands that still
/// return `Result<T, String>`.
impl From<AppError> for String {
    fn from(e: AppError) -> String {
        e.to_string()
    }
}

// ── Structured error for Tauri commands ───────────────────────────────────

/// Serializable error returned to the frontend by Tauri commands.
///
/// Provides a machine-readable `kind` so the UI can differentiate between
/// cancellation, timeouts, network errors, etc. without parsing strings.
#[derive(Debug, Serialize, Clone)]
pub struct SkillError {
    /// Machine-readable error category.
    pub kind: &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Whether the operation could succeed if retried.
    pub retryable: bool,
}

impl SkillError {
    /// Parse a raw error string (from legacy `Result<T, String>` paths) into
    /// a structured SkillError by matching known prefixes.
    pub fn from_raw(raw: &str) -> Self {
        if raw == "cancelled" {
            return Self {
                kind: "cancelled",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.contains("timed out") {
            return Self {
                kind: "timeout",
                message: raw.to_string(),
                retryable: true,
            };
        }
        if raw.starts_with("Failed to start") {
            return Self {
                kind: "tool_missing",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.starts_with("Claude error:") || raw.starts_with("Codex error:") {
            return Self {
                kind: "agent_error",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.contains("read-only run") {
            return Self {
                kind: "permission",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.starts_with("config error:") {
            return Self {
                kind: "config",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.starts_with("network error:") || raw.starts_with("API overloaded") {
            return Self {
                kind: "network",
                message: raw.to_string(),
                retryable: true,
            };
        }
        if raw.starts_with("API error") {
            return Self {
                kind: "api",
                message: raw.to_string(),
                retryable: false,
            };
        }
        if raw.starts_with("Unknown skill:") {
            return Self {
                kind: "invalid_mode",
                message: raw.to_string(),
                retryable: false,
            };
        }
        Self {
            kind: "internal",
            message: raw.to_string(),
            retryable: false,
        }
    }

    /// Convert from a typed `AppError`.
    pub fn from_app_error(e: &AppError) -> Self {
        let kind = match e {
            AppError::Cancelled => "cancelled",
            AppError::Config(_) => "config",
            AppError::Network(_) => "network",
            AppError::Api { .. } => "api",
            AppError::ApiOverloaded { .. } => "network",
            AppError::ApiParse(_) => "api_parse",
            AppError::Tool { .. } => "tool",
            AppError::Io(_) => "io",
            AppError::Merge(_) => "merge",
            AppError::Verification(_) => "verification",
            AppError::Internal(_) => "internal",
        };
        Self {
            kind,
            message: e.to_string(),
            retryable: e.is_retryable(),
        }
    }
}

impl fmt::Display for SkillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<AppError> for SkillError {
    fn from(e: AppError) -> Self {
        Self::from_app_error(&e)
    }
}

// ── Retry helper ───────────────────────────────────────────────────────────

/// Maximum number of retries for transient failures.
const MAX_RETRIES: u32 = 3;

/// Initial backoff duration in milliseconds.
const INITIAL_BACKOFF_MS: u64 = 1_000;

/// Execute an async operation with exponential backoff on retryable errors.
///
/// Retries up to `MAX_RETRIES` times with delays of 1s, 2s, 4s.
/// Non-retryable errors are returned immediately.
/// If `token` is provided, cancellation during backoff sleep is respected.
pub async fn with_retry<F, Fut, T>(
    operation: F,
    token: Option<&tokio_util::sync::CancellationToken>,
) -> Result<T, AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, AppError>>,
{
    let mut last_err = AppError::Internal("no attempts made".to_string());

    for attempt in 0..=MAX_RETRIES {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) if e.is_retryable() && attempt < MAX_RETRIES => {
                let delay = INITIAL_BACKOFF_MS * (1 << attempt);
                tracing::warn!(
                    attempt = attempt + 1,
                    max_attempts = MAX_RETRIES + 1,
                    error = %e,
                    delay_ms = delay,
                    "retrying after transient failure",
                );
                // Respect cancellation during backoff sleep so the user
                // doesn't have to wait up to 7s for retries to drain.
                let sleep = tokio::time::sleep(std::time::Duration::from_millis(delay));
                if let Some(tok) = token {
                    tokio::select! {
                        _ = tok.cancelled() => return Err(AppError::Cancelled),
                        _ = sleep => {},
                    }
                } else {
                    sleep.await;
                }
                last_err = e;
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_err)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_errors() {
        assert!(AppError::Network("timeout".into()).is_retryable());
        assert!(AppError::ApiOverloaded {
            status: 429,
            body: "rate limited".into()
        }
        .is_retryable());
        assert!(AppError::ApiOverloaded {
            status: 503,
            body: "service unavailable".into()
        }
        .is_retryable());
    }

    #[test]
    fn non_retryable_errors() {
        assert!(!AppError::Api {
            status: 400,
            body: "bad request".into()
        }
        .is_retryable());
        assert!(!AppError::Cancelled.is_retryable());
        assert!(!AppError::Config("missing key".into()).is_retryable());
        assert!(!AppError::Tool {
            tool: "bash".into(),
            detail: "not found".into()
        }
        .is_retryable());
    }

    #[test]
    fn from_api_status_overloaded() {
        let e = AppError::from_api_status(429, "rate limit".into());
        assert!(matches!(e, AppError::ApiOverloaded { status: 429, .. }));

        let e = AppError::from_api_status(500, "internal".into());
        assert!(matches!(e, AppError::ApiOverloaded { status: 500, .. }));
    }

    #[test]
    fn from_api_status_permanent() {
        let e = AppError::from_api_status(400, "bad request".into());
        assert!(matches!(e, AppError::Api { status: 400, .. }));

        let e = AppError::from_api_status(401, "unauthorized".into());
        assert!(matches!(e, AppError::Api { status: 401, .. }));
    }

    #[test]
    fn display_cancelled() {
        assert_eq!(AppError::Cancelled.to_string(), "cancelled");
    }

    #[tokio::test]
    async fn with_retry_succeeds_immediately() {
        let result = with_retry(|| async { Ok::<_, AppError>(42) }, None).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn with_retry_stops_on_non_retryable() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = with_retry(move || {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<(), _>(AppError::Api {
                    status: 400,
                    body: "bad".into(),
                })
            }
        }, None)
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    // ── SkillError parsing ───────────────────────────────────────────────

    #[test]
    fn skill_error_cancelled() {
        let e = SkillError::from_raw("cancelled");
        assert_eq!(e.kind, "cancelled");
        assert!(!e.retryable);
    }

    #[test]
    fn skill_error_timeout_is_retryable() {
        let e = SkillError::from_raw("codex timed out after 1800 s");
        assert_eq!(e.kind, "timeout");
        assert!(e.retryable);
    }

    #[test]
    fn skill_error_tool_missing() {
        let e = SkillError::from_raw("Failed to start `node`: No such file or directory");
        assert_eq!(e.kind, "tool_missing");
        assert!(!e.retryable);
    }

    #[test]
    fn skill_error_agent_error() {
        let e = SkillError::from_raw("Claude error: rate limit exceeded");
        assert_eq!(e.kind, "agent_error");
    }

    #[test]
    fn skill_error_network_retryable() {
        let e = SkillError::from_raw("network error: connection refused");
        assert_eq!(e.kind, "network");
        assert!(e.retryable);
    }

    #[test]
    fn skill_error_unknown_fallback() {
        let e = SkillError::from_raw("something unexpected happened");
        assert_eq!(e.kind, "internal");
        assert!(!e.retryable);
    }

    #[test]
    fn skill_error_from_app_error() {
        let e = SkillError::from_app_error(&AppError::Network("timeout".into()));
        assert_eq!(e.kind, "network");
        assert!(e.retryable);

        let e = SkillError::from_app_error(&AppError::Cancelled);
        assert_eq!(e.kind, "cancelled");
        assert!(!e.retryable);
    }

    #[test]
    fn skill_error_from_trait_conversion() {
        let app_err = AppError::Io("disk full".into());
        let skill_err: SkillError = app_err.into();
        assert_eq!(skill_err.kind, "io");
        assert!(!skill_err.retryable);
    }

    // ── Display coverage ─────────────────────────────────────────────────

    #[test]
    fn display_all_variants() {
        let cases: Vec<(AppError, &str)> = vec![
            (AppError::Cancelled, "cancelled"),
            (AppError::Config("bad".into()), "config error: bad"),
            (
                AppError::Network("timeout".into()),
                "network error: timeout",
            ),
            (
                AppError::Api {
                    status: 400,
                    body: "bad".into(),
                },
                "API error 400: bad",
            ),
            (
                AppError::ApiOverloaded {
                    status: 429,
                    body: "slow".into(),
                },
                "API overloaded 429: slow",
            ),
            (AppError::ApiParse("json".into()), "API parse error: json"),
            (
                AppError::Tool {
                    tool: "bash".into(),
                    detail: "fail".into(),
                },
                "bash: fail",
            ),
            (AppError::Io("disk".into()), "IO error: disk"),
            (AppError::Merge("conflict".into()), "merge error: conflict"),
            (
                AppError::Verification("mismatch".into()),
                "verification failed: mismatch",
            ),
            (AppError::Internal("oops".into()), "internal error: oops"),
        ];

        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected);
        }
    }

    // ── From conversions ─────────────────────────────────────────────────

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let app_err = AppError::from(io_err);
        assert!(matches!(app_err, AppError::Io(_)));
        assert!(app_err.to_string().contains("not found"));
    }

    #[test]
    fn from_serde_error() {
        let serde_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let app_err = AppError::from(serde_err);
        assert!(matches!(app_err, AppError::ApiParse(_)));
    }

    #[test]
    fn app_error_to_string_conversion() {
        let err = AppError::Config("missing key".into());
        let s: String = err.into();
        assert_eq!(s, "config error: missing key");
    }

    // ── SkillError coverage for all parse branches ───────────────────────

    #[test]
    fn skill_error_permission() {
        let e = SkillError::from_raw("Claude read-only run attempted forbidden tool");
        assert_eq!(e.kind, "permission");
    }

    #[test]
    fn skill_error_config() {
        let e = SkillError::from_raw("config error: missing API key");
        assert_eq!(e.kind, "config");
    }

    #[test]
    fn skill_error_api_overloaded() {
        let e = SkillError::from_raw("API overloaded 429: rate limit");
        assert_eq!(e.kind, "network");
        assert!(e.retryable);
    }

    #[test]
    fn skill_error_api_error() {
        let e = SkillError::from_raw("API error 400: bad request");
        assert_eq!(e.kind, "api");
        assert!(!e.retryable);
    }

    #[test]
    fn skill_error_invalid_mode() {
        let e = SkillError::from_raw("Unknown skill: fly");
        assert_eq!(e.kind, "invalid_mode");
    }

    // ── SkillError from all AppError variants ────────────────────────────

    #[test]
    fn skill_error_covers_all_app_error_variants() {
        let cases: Vec<(AppError, &str)> = vec![
            (AppError::Cancelled, "cancelled"),
            (AppError::Config("x".into()), "config"),
            (AppError::Network("x".into()), "network"),
            (
                AppError::Api {
                    status: 400,
                    body: "x".into(),
                },
                "api",
            ),
            (
                AppError::ApiOverloaded {
                    status: 429,
                    body: "x".into(),
                },
                "network",
            ),
            (AppError::ApiParse("x".into()), "api_parse"),
            (
                AppError::Tool {
                    tool: "bash".into(),
                    detail: "x".into(),
                },
                "tool",
            ),
            (AppError::Io("x".into()), "io"),
            (AppError::Merge("x".into()), "merge"),
            (AppError::Verification("x".into()), "verification"),
            (AppError::Internal("x".into()), "internal"),
        ];

        for (app_err, expected_kind) in cases {
            let skill_err = SkillError::from_app_error(&app_err);
            assert_eq!(skill_err.kind, expected_kind, "Failed for {app_err:?}");
        }
    }
}
