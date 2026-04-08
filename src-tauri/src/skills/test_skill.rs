use super::{runners, ReviewPhaseResult, ToolLog};
/// Test skill — full integration test pipeline.
///
/// Phases (orchestrated by the frontend runTest()):
///   gen_test_plan    — Claude + Codex parallel, read PLAN.md, produce test.md
///   frontend_test    — Claude drives browser/playwright to test the UI
///   integration_test — env setup, server start on a free port (47000-47099), curl suite (A-G)
///   fix              — Claude reads bugs.md and applies fixes, marks items [x]
///   codex_fix        — Codex escalation reading bugs.md if Claude's fix failed
///   document         — Claude generates the Chinese project completion report
///
/// Each phase emits "review-phase-result" when it finishes.
/// The "document" phase additionally emits "completion-report" with the full markdown.
use crate::{
    evidence::{self, EvidenceEvent},
    planning_schema::{read_plan_acceptance_lenient, PLAN_ACCEPTANCE_JSON},
};
use chrono::Utc;
use tauri::{Emitter, EventTarget};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub(crate) struct TestRuntimePaths {
    pub pid_path: String,
    pub log_path: String,
}

pub(super) async fn run_phase(
    phase: &str,
    task: &str,
    issue: Option<&str>,
    workspace: Option<&str>,
    context: Option<&str>,
    window_label: &str,
    app_handle: &tauri::AppHandle,
    token: CancellationToken,
) -> Result<(), String> {
    let runtime = prepare_runtime_paths(window_label, workspace)?;
    let (acceptance, acceptance_warning) = workspace
        .map(read_plan_acceptance_lenient)
        .unwrap_or((None, None));
    let acceptance_section = acceptance.map(|acceptance| {
        let json = serde_json::to_string_pretty(&acceptance).unwrap_or_else(|_| "{}".to_string());
        format!("## Structured Acceptance ({PLAN_ACCEPTANCE_JSON})\n\n```json\n{json}\n```")
    });
    if let Some(warning) = &acceptance_warning {
        emit_acceptance_warning_log(app_handle, window_label, warning)?;
    }
    let warning_section = acceptance_warning.map(|warning| {
        format!(
            "## Structured Acceptance Warning\n\n{PLAN_ACCEPTANCE_JSON} could not be used. Continue with fallback test criteria only.\n\nReason: {warning}"
        )
    });
    let merged_context = super::merge_context_sections(&[
        context.map(ToOwned::to_owned),
        warning_section,
        acceptance_section,
    ]);
    let context = merged_context.as_deref();

    let (passed, found_issue) = match phase {
        // ── Phase 0: Generate test.md — Claude + Codex parallel ───────────────
        "gen_test_plan" => {
            let claude_prompt = format!(
                "You are a senior QA architect. Read PLAN.md (and any source files needed) \
                 in the current directory and produce a comprehensive test plan document.\n\
                 Task context: {task}\n\n\
                 Steps:\n\
                 1. Read PLAN.md. List every backend feature (F-items) and UI screen (P-items).\n\
                 2. For each feature/screen design concrete test cases covering:\n\
                    - Happy path\n\
                    - Edge cases (empty, boundary values, max/min)\n\
                    - Error cases (invalid input, missing auth, not found)\n\
                    - Security (injection, unauthorised access)\n\
                 3. For frontend screens: design browser interaction steps \
                    (navigate, fill form, click, assert DOM element / text visible).\n\
                 4. Write your test plan to `test.md` in this format:\n\n\
                 # Test Plan\n\n\
                 ## Backend Tests\n\
                 ### F1. <Feature Name>\n\
                 - [ ] TC-F1-1: <test case description> | Method: POST /path | Input: ... | Expected: 201\n\
                 - [ ] TC-F1-2: ...\n\n\
                 ## Frontend Tests\n\
                 ### P1. <Screen Name>\n\
                 - [ ] TC-P1-1: <step-by-step browser test> | Assert: <what should be visible>\n\n\
                 Use the Write tool to create test.md. Split across multiple Edits if needed (~2000 chars each).\n\
                 At the very end output: TESTPLAN_DONE"
            );
            let codex_prompt = format!(
                "You are a senior QA engineer. Read PLAN.md in the current directory.\n\
                 Task context: {task}\n\n\
                 Independently review the plan and list any test scenarios the architect might miss:\n\
                 - Concurrency / race conditions\n\
                 - Data integrity across operations\n\
                 - Auth token edge cases (expiry, revocation)\n\
                 - Frontend state bugs (stale data, loading states, error messages)\n\
                 - Missing validations or missing error responses\n\n\
                 Format your additions as:\n\
                 ## Codex Additional Test Cases\n\
                 ### <Feature/Screen>\n\
                 - [ ] TC-CX-1: <description> | Expected: <result>\n\n\
                 At the very end output: CODEX_DONE"
            );

            // Inject plan context into both prompts
            let claude_prompt = super::inject_context(context, claude_prompt);
            let codex_prompt = super::inject_context(context, codex_prompt);

            // Run both in parallel
            let (claude_res, codex_res) = tokio::join!(
                runners::claude(
                    &claude_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone()
                ),
                runners::codex_read_only(
                    &codex_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone()
                ),
            );
            let claude_out = claude_res?;
            let codex_out = codex_res?;

            // Merge Codex additions into test.md
            let merge_prompt = format!(
                "test.md has been written by Claude. Now append the following additional test cases \
                 from Codex into test.md — add them under the existing sections or create new sections \
                 as appropriate. Do not duplicate items already present.\n\n\
                 Codex additions:\n{codex_out}\n\n\
                 Claude confirmation:\n{claude_out}\n\n\
                 Use the Edit tool to append to test.md. \
                 At the very end output: [RESULT:PASS]"
            );
            let merge_prompt = super::inject_context(context, merge_prompt);
            parse_result(
                &runners::claude(
                    &merge_prompt,
                    workspace,
                    window_label,
                    app_handle,
                    token.clone(),
                )
                .await?,
            )
        }

        // ── Phase 1: UI tests — project-type-aware ────────────────────────────
        "frontend_test" => {
            let prompt = format!(
                "You are a QA engineer running UI tests for: {task}\n\n\
                 ═══ STEP 1: IDENTIFY PROJECT TYPE ═══\n\
                 Inspect the project files to determine the exact project type.\n\
                 Check in order:\n\
                 - package.json containing react/vue/svelte/next/nuxt/angular → TYPE: web\n\
                 - pubspec.yaml → TYPE: flutter\n\
                 - build.gradle or settings.gradle → TYPE: android\n\
                 - *.xcodeproj or *.xcworkspace → TYPE: ios\n\
                 - Cargo.toml containing tauri/iced/egui/druid → TYPE: desktop-rust\n\
                 - CMakeLists.txt or *.pro (Qt) → TYPE: desktop-qt\n\
                 - *.csproj containing MAUI/WPF/WinForms → TYPE: desktop-dotnet\n\
                 - No UI files at all → TYPE: no-ui\n\
                 Print: PROJECT TYPE: <type>\n\n\
                 ═══ STEP 2: READ TEST PLAN ═══\n\
                 Read test.md. Extract all TC-P-* entries.\n\
                 If test.md does not exist or has no TC-P-* items:\n\
                   print 'No UI test cases in test.md — skipping.' and output [RESULT:PASS]\n\n\
                 ═══ STEP 3: EXECUTE TESTS BY PROJECT TYPE ═══\n\n\
                 TYPE web:\n\
                   Run: npx playwright --version 2>/dev/null\n\
                   If not found: npm install -D @playwright/test && npx playwright install chromium\n\
                   For each TC-P-* write and run a self-contained playwright script:\n\
                     run `lsof -iTCP -sTCP:LISTEN | grep 47` to find the FREE_PORT the server is on,\n\
                     launch headless Chromium, navigate to http://localhost:<FREE_PORT>, execute steps, assert DOM state.\n\
                   Record each as PASS or FAIL with error details.\n\n\
                 TYPE flutter:\n\
                   Run: flutter --version 2>/dev/null\n\
                   If available: write widget tests in test/widget_test.dart, run: flutter test\n\
                   If not available: cannot execute — go to Step 4.\n\n\
                 TYPE android:\n\
                   Run: adb devices\n\
                   If a device/emulator is listed: write Espresso tests, run: ./gradlew connectedAndroidTest\n\
                   If no device: cannot execute — go to Step 4.\n\n\
                 TYPE ios:\n\
                   Run: xcrun simctl list devices booted\n\
                   If a booted simulator exists: write XCTest UI tests,\n\
                     run: xcodebuild test -scheme <scheme> -destination 'platform=iOS Simulator,...'\n\
                   If no simulator: cannot execute — go to Step 4.\n\n\
                 TYPE desktop-rust / desktop-qt / desktop-dotnet:\n\
                   Run: appium --version 2>/dev/null\n\
                   If available: write Appium scripts with the appropriate driver, run them.\n\
                   If not available: cannot execute — go to Step 4.\n\n\
                 TYPE no-ui:\n\
                   print 'No UI layer — skipping.' and output [RESULT:PASS]\n\n\
                 ═══ STEP 4: MANUAL CHECKLIST (automation unavailable) ═══\n\
                 Only enter this step when required tooling or device is missing.\n\
                 Append to test.md under a new section:\n\
                 ## Manual UI Test Checklist\n\
                 > Automation unavailable: <exact reason e.g. 'no booted iOS simulator'>\n\
                 - [ ] TC-P1-1: <exact steps a human tester should follow> | Assert: <what to verify>\n\
                 (one entry per TC-P-* item — specific enough to follow without guessing)\n\
                 Then output [RESULT:PASS] — missing tooling is not a test failure.\n\n\
                 ═══ STEP 5: RECORD RESULTS ═══\n\
                 For every automated FAIL: append to bugs.md (create if not exists):\n\
                 # Bug Report — UI Tests\n\
                 - [ ] **<TC-ID>** — <description> | Error: <actual error> | Tool: <tool used>\n\
                 In test.md: change `- [ ]` to `- [x]` for every automated PASS.\n\n\
                 Print a summary table:\n\
                 | TC | Description | Tool | Result |\n\
                 |----|-------------|------|--------|\n\n\
                 At the very end append:\n\
                 [RESULT:PASS] if all executed tests passed, or tooling unavailable (manual checklist generated)\n\
                 [RESULT:FAIL:<TC-ID> reason, <TC-ID> reason] list only automated failures"
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        // ── Full integration test suite ───────────────────────────────────────
        "integration_test" => {
            cleanup_runtime_process(&runtime);
            let prompt = format!(
                "You are a senior QA engineer running comprehensive integration tests for: {task}\n\n\
                 ═══ PHASE A: UNDERSTAND, BUILD, AND START ═══\n\n\
                 A1. READ THE PROJECT\n\
                     Inspect all config and manifest files present in the working directory\n\
                     (e.g. package.json, pubspec.yaml, build.gradle, go.mod, Cargo.toml,\n\
                     pyproject.toml, requirements.txt, *.xcodeproj, CMakeLists.txt, etc.).\n\
                     In 2–3 sentences describe:\n\
                     - What is this project? (web app / mobile app / backend API / desktop app / CLI / library)\n\
                     - What build tool does it use?\n\
                     - What does \"ready to verify\" look like for this specific project?\n\
                       (e.g. \"server listening on port\", \"APK at path X\", \"binary compiled to Y\")\n\
                     Print: 「Project: <your 2–3 sentence description>」\n\n\
                 A2. INSTALL DEPENDENCIES & BUILD\n\
                     Use the build tool you identified in A1 to:\n\
                     1. Install dependencies (e.g. npm install / pip install / flutter pub get /\n\
                        go mod download / pod install / cargo build)\n\
                     2. Build for the most testable target:\n\
                        - If the project has a web/browser build option, prefer it — it enables curl testing\n\
                        - If not, build the native artifact (APK, binary, packaged app, etc.)\n\
                     If the build fails: read the error, fix the root cause in the source, retry (max 2 attempts).\n\
                     Print the build result and the output artifact path.\n\n\
                 A3. RUN THE PROJECT'S OWN TEST SUITE (if one exists)\n\
                     Check for built-in test commands (e.g. npm test, pytest, go test, flutter test,\n\
                     ./gradlew test, cargo test). If found, run it and print the summary.\n\
                     If none exists: print 「No test suite found — skipping」\n\n\
                 A4. FIND A FREE PORT\n\
                     Run the following shell snippet to find the first available port in the\n\
                     47000–47099 range. Do NOT hardcode any port number.\n\
                     Do NOT kill any existing process — only find a port that is already free.\n\
                     ```sh\n\
                     for PORT in $(seq 47000 47099); do\n\
                       if ! lsof -iTCP:$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then\n\
                         echo \"FREE_PORT=$PORT\"; break;\n\
                       fi\n\
                     done\n\
                     ```\n\
                     If no free port is found in the range: print an error and output [RESULT:FAIL:no free port in 47000-47099]\n\
                     Save the found port as FREE_PORT for all subsequent steps.\n\
                     Print: 「✅ FREE_PORT: <value>」\n\n\
                 A5. MAKE THE PROJECT ACCESSIBLE FOR VERIFICATION\n\
                     Based on A1, choose the appropriate exposure method:\n\
                     - Server-based project (web app, API, backend):\n\
                       Start on FREE_PORT as a background process → {server_log_path} + {server_pid_path}\n\
                       Retry `curl -sf http://localhost:$FREE_PORT` every 2 s, up to 30 s.\n\
                       If not ready: cat {server_log_path}, diagnose, fix, restart (max 3 attempts).\n\
                       Set ACCESS_ENTRY=\"http://localhost:$FREE_PORT\"\n\
                     - Project with a web export (browser-capable build in a static directory):\n\
                       Serve that directory on FREE_PORT: npx serve <dir> -p $FREE_PORT -s\n\
                       Wait for readiness the same way. Set ACCESS_ENTRY=\"http://localhost:$FREE_PORT\"\n\
                     - Native installable artifact (APK, IPA, binary, desktop package):\n\
                       No HTTP server. Record the artifact path and the run/install command.\n\
                       If the repo also contains a backend, start that on FREE_PORT and run curl tests on it.\n\
                       Set ACCESS_ENTRY=\"<artifact path> — install/run with: <command>\"\n\
                     Print: 「✅ ACCESS_ENTRY: <value>」\n\n\
                 ═══ PHASE B: FEATURE INVENTORY ═══\n\
                 ⚠️  If the project produces only a native artifact (no HTTP server was started):\n\
                     Skip Phases C–F entirely.\n\
                     Go directly to Phase G and report: build status, test results, artifact path.\n\
                     Set [RESULT:PASS] if build succeeded and own tests passed.\n\
                     Set [RESULT:FAIL:reason] if build failed or tests failed.\n\n\
                 For all other projects (or native apps that also have a backend), continue:\n\n\
                 B1. READ PLAN.md (if it exists).\n\
                     Extract every planned feature, module, endpoint, and deliverable.\n\
                     Mark each as: [ ] <feature name>\n\n\
                 B2. SCAN ALL route/controller/handler files in the source code.\n\
                     List every implemented endpoint: METHOD  PATH  description\n\n\
                 B3. CROSS-REFERENCE:\n\
                     - For each planned feature from B1, find the matching endpoint(s) from B2.\n\
                     - Mark as ✅ COVERED if found, or ⚠️ NOT FOUND if missing from code.\n\
                     - Print the full merged checklist before continuing.\n\
                     - IMPORTANT: you must test every item in this checklist — do not skip\n\
                       any feature just because it was not discovered through route scanning.\n\n\
                 ═══ PHASE C: AUTHENTICATION FLOW ═══\n\
                 If auth endpoints exist:\n\
                   C1. Register new test user (if endpoint exists) → expect 201\n\
                   C2. Login with valid credentials → save TOKEN from response\n\
                   C3. Login with wrong password → expect 401/400\n\
                   C4. Login with non-existent user → expect 401/404\n\
                   C5. Access protected endpoint with NO token → expect 401/403\n\
                   C6. Access protected endpoint with INVALID token → expect 401\n\
                   C7. Access protected endpoint with valid token → expect 200\n\n\
                 ═══ PHASE D: CRUD TESTING (for every resource) ═══\n\
                 For each resource (users, items, orders, candidates, etc.):\n\
                   D1. CREATE  POST   valid payload          → expect 201/200; save created ID\n\
                   D2. CREATE  POST   missing required field → expect 422/400\n\
                   D3. CREATE  POST   wrong field type       → expect 422/400\n\
                   D4. CREATE  POST   duplicate (if unique)  → expect 409/400\n\
                   D5. LIST    GET    /                      → expect 200, array response\n\
                   D6. GET     GET    /{{id}}                → expect 200\n\
                   D7. GET     GET    /99999 (not found)     → expect 404\n\
                   D8. UPDATE  PUT    /{{id}} valid data      → expect 200\n\
                   D9. UPDATE  PATCH  /{{id}} partial data    → expect 200 (if endpoint exists)\n\
                   D10.UPDATE  PUT    /99999                 → expect 404\n\
                   D11.DELETE  DELETE /{{id}}                → expect 200/204\n\
                   D12.DELETE  DELETE /{{id}} again          → expect 404\n\n\
                 ═══ PHASE E: SECURITY TESTS ═══\n\
                   E1. SQL injection in query params: ?q=' OR 1=1--  → must NOT return 500\n\
                   E2. SQL injection in path: /users/1' OR '1'='1   → must NOT return 500\n\
                   E3. XSS payload in body string field: \"<script>alert(1)</script>\"\n\
                   E4. Oversized payload: send 10MB body → must NOT crash server\n\
                   E5. Tampered JWT: modify token payload, re-send → expect 401\n\
                   E6. Access another user's private resource with different token → expect 403/404\n\
                   E7. Missing Content-Type header on POST → expect 422/400 not 500\n\n\
                 ═══ PHASE F: EDGE & BOUNDARY CASES ═══\n\
                   F1. Empty list endpoint → expect 200 with empty array (not 404)\n\
                   F2. Pagination: ?page=0, ?page=-1, ?limit=99999\n\
                   F3. Search/filter with special chars: ?q=%00, ?q=../../etc/passwd\n\
                   F4. Concurrent identical POSTs (run 3 curl in parallel with &) → no duplicates\n\
                   F5. Very long string (1000 chars) in text fields\n\n\
                 ═══ PHASE G: RECORD RESULTS ═══\n\
                 G1. Full test result table:\n\
                 | Phase | Test | Endpoint | Expected | Actual | Status |\n\
                 |-------|------|----------|----------|--------|--------|\n\
                 Fill in every row for every test run above.\n\n\
                 G2. Plan coverage summary (cross-reference with Phase B checklist):\n\
                 | Planned Feature | Endpoint(s) | Tested | Result |\n\
                 |-----------------|-------------|--------|--------|\n\
                 List every feature from PLAN.md (or every discovered feature if no PLAN.md).\n\
                 Mark untested features explicitly as ❌ NOT TESTED.\n\n\
                 ⚠️  DO NOT kill the server. Leave it running on FREE_PORT for the document phase.\n\n\
                 ═══ PHASE H: WRITE bugs.md IF ANY FAILURES ═══\n\
                 If ANY test in phases C–F failed:\n\
                 Write (or append to) `bugs.md` in the current directory using this format:\n\n\
                 # Bug Report — Integration Tests\n\
                 ## [<current datetime>] {task}\n\
                 - [ ] **<TC-ID>** — <test description> | Expected: <expected> | Actual: <actual> | Endpoint: <method path>\n\
                 (one line per failed test case — be specific, include all failures, not a summary)\n\n\
                 Mark test.md items: change `- [ ]` to `- [x]` for every passed TC-F-* or TC-CX-* item.\n\n\
                 At the very end append exactly one of:\n\
                 [RESULT:PASS] if server is running and all planned/critical features work correctly\n\
                 [RESULT:FAIL:description] if server could not start, or any planned feature is broken or untested",
                server_log_path = runtime.log_path,
                server_pid_path = runtime.pid_path,
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        // ── Runtime fix (Claude) — reads full bugs.md ────────────────────────
        "fix" => {
            let issue_desc = issue.unwrap_or("test failure");
            let prompt = format!(
                "Fix all bugs found during testing for: {task}\n\n\
                 Summary of failure passed by orchestrator: {issue_desc}\n\n\
                 Steps:\n\
                 1. Read `bugs.md` in the current directory.\n\
                    This file contains the complete list of all failed test cases.\n\
                    If bugs.md does not exist, use the summary above as the issue description.\n\n\
                 2. For each unresolved item (`- [ ]`) in bugs.md:\n\
                    a. Locate the exact file and line causing the failure.\n\
                    b. Check {server_log_path} if the issue is a server startup failure.\n\
                    c. Fix the root cause in the source code.\n\
                    d. After fixing, change `- [ ]` to `- [x]` for that item in bugs.md.\n\n\
                 3. Do NOT suppress errors — fix the underlying problem.\n\
                 4. If deps are missing, add them to requirements.txt / package.json and reinstall.\n\n\
                 Print a summary of every bug and whether it was fixed.\n\
                 At the very end append exactly one of:\n\
                 [RESULT:PASS] if all bugs in bugs.md are now fixed\n\
                 [RESULT:FAIL:remaining unfixed items] if any remain unresolved",
                server_log_path = runtime.log_path,
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        // ── Runtime fix escalation (Codex) — reads full bugs.md ──────────────
        "codex_fix" => {
            let issue_desc = issue.unwrap_or("test failure");
            let prompt = format!(
                "Escalation: Claude could not fully fix the bugs. Task: {task}\n\n\
                 Summary: {issue_desc}\n\n\
                 Steps:\n\
                 1. Read `bugs.md` in the current directory for the complete bug list.\n\
                 2. For each unresolved item (`- [ ]`), independently analyze and fix it.\n\
                 3. After fixing each item, change `- [ ]` to `- [x]` in bugs.md.\n\
                 4. Apply robust, root-cause fixes — not workarounds.\n\n\
                 At the very end append: [RESULT:PASS] or [RESULT:FAIL:remaining items]"
            );
            let prompt = super::inject_context(context, prompt);
            parse_result(
                &runners::codex(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?,
            )
        }

        // ── Project completion document ────────────────────────────────────────
        "document" => {
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

            // Run Claude — it writes the report to disk, text output is just [RESULT:PASS]
            let prompt = super::inject_context(context, prompt);
            let result_line =
                runners::claude(&prompt, workspace, window_label, app_handle, token.clone())
                    .await?;

            // Read the file Claude wrote; fall back to empty if somehow not written
            let report_content = std::fs::read_to_string(&report_path).unwrap_or_default();

            // Emit to UI so it appears in the chat (scoped to the requesting window)
            if !report_content.is_empty() {
                app_handle
                    .emit_to(
                        EventTarget::webview_window(window_label),
                        "completion-report",
                        &report_content,
                    )
                    .map_err(|e| format!("Emit error: {e}"))?;
            }

            parse_result(&result_line)
        }

        unknown => return Err(format!("Unknown test phase: {unknown}")),
    };

    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "review-phase-result",
            ReviewPhaseResult {
                phase: phase.to_string(),
                passed,
                issue: found_issue.clone(),
            },
        )
        .map_err(|e| e.to_string())?;
    if let Some(workspace) = workspace {
        evidence::record_event(
            workspace,
            EvidenceEvent {
                ts: Utc::now().timestamp_millis() as u64,
                event_type: format!("test_{phase}_{}", if passed { "passed" } else { "failed" }),
                agent: "system".to_string(),
                subtask_id: None,
                summary: if found_issue.trim().is_empty() {
                    format!("Test phase {phase} completed successfully.")
                } else {
                    format!("Test phase {phase} completed with issue: {found_issue}")
                },
                artifacts: artifacts_for_test_phase(phase),
            },
        )?;
    }

    Ok(())
}

fn emit_acceptance_warning_log(
    app_handle: &tauri::AppHandle,
    window_label: &str,
    warning: &str,
) -> Result<(), String> {
    app_handle
        .emit_to(
            EventTarget::webview_window(window_label),
            "tool-log",
            ToolLog {
                agent: "system".to_string(),
                tool: "StructuredAcceptance".to_string(),
                input: format!("Fallback active: {warning}"),
                timestamp: Utc::now().timestamp_millis() as u64,
            },
        )
        .map_err(|e| e.to_string())
}

fn artifacts_for_test_phase(phase: &str) -> Vec<String> {
    match phase {
        "gen_test_plan" => vec!["test.md".to_string(), "PLAN.md".to_string()],
        "frontend_test" | "integration_test" | "fix" | "codex_fix" => {
            vec!["test.md".to_string(), "bugs.md".to_string()]
        }
        "document" => vec!["PROJECT_REPORT.md".to_string(), "test.md".to_string()],
        _ => Vec::new(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn prepare_runtime_paths(
    window_label: &str,
    workspace: Option<&str>,
) -> Result<TestRuntimePaths, String> {
    let base_dir = match workspace.filter(|path| !path.trim().is_empty()) {
        Some(ws) => std::path::PathBuf::from(ws)
            .join(".ai-dev-hub")
            .join("runtime"),
        None => std::env::temp_dir().join("ai-dev-hub-runtime"),
    };
    std::fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Cannot create test runtime dir {}: {e}", base_dir.display()))?;

    let label = sanitize_runtime_label(window_label);
    Ok(TestRuntimePaths {
        pid_path: base_dir
            .join(format!("{label}.server.pid"))
            .to_string_lossy()
            .into_owned(),
        log_path: base_dir
            .join(format!("{label}.server.log"))
            .to_string_lossy()
            .into_owned(),
    })
}

pub(crate) fn cleanup_runtime_for_window(
    window_label: &str,
    workspace: Option<&str>,
) -> Result<(), String> {
    let runtime = prepare_runtime_paths(window_label, workspace)?;
    cleanup_runtime_process(&runtime);
    Ok(())
}

fn cleanup_runtime_process(runtime: &TestRuntimePaths) {
    let pid_path = std::path::Path::new(&runtime.pid_path);
    if let Ok(pid_text) = std::fs::read_to_string(pid_path) {
        let pid = pid_text.trim();
        if !pid.is_empty() {
            if cfg!(windows) {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", pid, "/T", "/F"])
                    .status();
            } else {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", pid])
                    .status();
            }
        }
    }

    let _ = std::fs::remove_file(&runtime.pid_path);
    let _ = std::fs::remove_file(&runtime.log_path);
}

fn sanitize_runtime_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let collapsed = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "default-window".to_string()
    } else {
        collapsed
    }
}

fn parse_result(text: &str) -> (bool, String) {
    if let Some(pos) = text.rfind("[RESULT:") {
        let suffix = &text[pos..];
        if suffix.starts_with("[RESULT:PASS]") {
            return (true, String::new());
        }
        if suffix.starts_with("[RESULT:FAIL:") {
            let issue = suffix
                .trim_start_matches("[RESULT:FAIL:")
                .splitn(2, ']')
                .next()
                .unwrap_or("unknown issue")
                .to_string();
            return (false, issue);
        }
        // [RESULT:...] found but neither PASS nor FAIL:reason — treat as failure
        return (
            false,
            format!(
                "malformed result marker: {}",
                &suffix[..suffix.len().min(40)]
            ),
        );
    }
    // No marker at all — output was likely truncated or agent skipped it
    (
        false,
        "no [RESULT:*] marker found — output may be truncated".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_runtime_paths_uses_hidden_workspace_runtime_dir() {
        let dir = tempfile::tempdir().unwrap();
        let runtime =
            prepare_runtime_paths("aidevchat-123", Some(dir.path().to_str().unwrap())).unwrap();
        assert!(runtime.pid_path.contains(".ai-dev-hub/runtime"));
        assert!(runtime.log_path.ends_with("aidevchat-123.server.log"));
    }

    #[test]
    fn sanitize_runtime_label_replaces_special_chars() {
        assert_eq!(
            sanitize_runtime_label("aidevchat:demo/1"),
            "aidevchat-demo-1"
        );
    }
}
