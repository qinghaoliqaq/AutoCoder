/// Document skill — generate the final Project Completion Report.
///
/// Runs after test passes. Reads the source tree, detects the running
/// service (if any), and writes a Chinese markdown report to
/// `<workspace>/PROJECT_REPORT.md`. The file is then emitted to the
/// frontend via the "completion-report" Tauri event so it appears as a
/// report bubble in the chat.
use super::evidence::{self, EvidenceEvent};
use crate::{config::AppConfig, tool_runner};
use chrono::Utc;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    task: &str,
    workspace: Option<&str>,
    context: Option<&str>,
    config: &AppConfig,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let sys_write = "You are a senior technical writer. \
                     Use the editor and bash tools to read the codebase \
                     and write a faithful project completion report.";

    let report_path = workspace
        .map(|ws| format!("{ws}/PROJECT_REPORT.md"))
        .unwrap_or_else(|| "/tmp/PROJECT_REPORT.md".to_string());

    let prompt = format!(
        "Generate a comprehensive Project Completion Report for: {task}\n\n\
         BEFORE writing the report:\n\
         1. Read the source code and all config files so every command you write is accurate.\n\
         2. Run `lsof -iTCP -sTCP:LISTEN | grep 47` to find which port the server is using,\n\
            then try `curl -s http://localhost:<that port>` — note whether a service is currently reachable.\n\
         3. Based on what you read, determine the real access method for THIS specific project.\n\
            Write ONLY what is true — do not copy a generic template.\n\n\
         IMPORTANT: Write the report to `{report_path}` using your Write and Edit tools.\n\
         Each single Write or Edit call must contain AT MOST ~2000 characters of new content.\n\
         If a section exceeds ~2000 characters, split it across multiple consecutive Edits.\n\
         Build in order: skeleton (headings only) → fill each section one by one.\n\n\
         The report must be in Chinese markdown with this structure:\n\n\
         # 项目完成报告\n\
         > 任务: {task}\n\
         > 生成时间: (当前日期时间)\n\n\
         ---\n\n\
         ## 立即使用\n\
         根据你对本项目的实际了解，写出真实的访问方式（不要套用模板，只写适用的内容）。\n\n\
         ---\n\n\
         ## 一、项目概述\n\
         简述项目功能、技术栈（语言/框架/数据库）、目录结构（树形或列表）。\n\n\
         ## 二、已实现功能清单\n\
         | 模块 | 功能 | 实现状态 | 说明 |\n\
         |------|------|----------|------|\n\
         逐条列出所有已实现功能，与 PLAN.md 中的计划一一对应。\n\n\
         ## 三、API 端点总览\n\
         | 方法 | 路径 | 功能描述 | 需要认证 | curl 示例 |\n\
         |------|------|----------|----------|-----------|\n\
         列出每一个端点，curl 示例必须可直接复制执行。\n\n\
         ## 四、测试结果汇总\n\
         | 测试类别 | 测试项 | 端点 | 期望状态 | 实际状态 | 结果 |\n\
         |----------|--------|------|----------|----------|------|\n\
         从集成测试阶段的输出中提取所有测试行，完整填写此表。\n\n\
         ## 五、安全性评估\n\
         逐条说明 SQL 注入 / XSS / 未授权访问 / JWT 篡改 等测试结果。\n\n\
         ## 六、已知问题 & 待改进\n\
         诚实列出测试中发现的问题或未覆盖的场景，若无则写「无」。\n\n\
         ## 七、本地启动指南\n\
         写出真实、可直接复制执行的命令（根据实际项目类型填写）：\n\
         ```bash\n\
         cd <实际项目目录名>\n\
         <实际安装依赖命令>\n\
         <实际启动命令>\n\
         open http://localhost:<实际端口>\n\
         ```\n\n\
         ## 八、可用性结论\n\
         给出项目整体评分（优 / 良 / 待改进）和一句话结论。\n\n\
         After writing the complete file, output exactly: [RESULT:PASS]"
    );

    let prompt = super::inject_context(context, prompt);
    let _ = tool_runner::run(
        config,
        sys_write,
        &prompt,
        workspace,
        window_label,
        app_handle,
        token.clone(),
    )
    .await?;

    // Read the file the agent wrote; fall back to empty if somehow not written.
    let report_content = std::fs::read_to_string(&report_path).unwrap_or_default();

    // Emit to UI so it appears in the chat (scoped to the requesting window).
    if !report_content.is_empty() {
        let _ = app_handle.emit_to(
            EventTarget::webview_window(window_label),
            "completion-report",
            &report_content,
        );
    }

    // Evidence recording is best-effort — must never fail the document skill.
    if let Some(ws) = workspace {
        let summary = if report_content.is_empty() {
            "document skill: PROJECT_REPORT.md was not generated".to_string()
        } else {
            "document skill: PROJECT_REPORT.md written".to_string()
        };
        let _ = evidence::record_event(
            ws,
            EvidenceEvent {
                ts: Utc::now().timestamp_millis().max(0) as u64,
                event_type: "document_completed".to_string(),
                agent: "system".to_string(),
                subtask_id: None,
                summary,
                artifacts: vec!["PROJECT_REPORT.md".to_string()],
            },
        );
    }

    Ok(())
}
