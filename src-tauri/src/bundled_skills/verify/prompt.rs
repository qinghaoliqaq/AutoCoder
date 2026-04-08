pub const PROMPT: &str = r#"# Verify — End-to-End Verification of Code Changes

Verify that recent code changes actually work by running the application
and testing the affected functionality.

## Approach

### Step 1: Identify What Changed

- Read recent git diff or changed files to understand the scope of changes
- Identify which features, endpoints, or UI components were modified
- Determine what needs to be tested

### Step 2: Build & Compile

- Run the project's build command (cargo build, npm run build, etc.)
- Fix any compilation or type errors
- Ensure the build completes cleanly with no warnings related to changes

### Step 3: Run Existing Tests

- Execute the project's test suite (cargo test, npm test, etc.)
- If specific test files exist for changed modules, run those first
- Report any test failures — do NOT skip or ignore them

### Step 4: Manual Verification

For backend changes:
- Start the server if applicable
- Use curl/fetch to test affected API endpoints
- Verify request/response shapes match expectations
- Test error cases (invalid input, auth failures, not found)

For frontend changes:
- Check that the dev server starts without errors
- Verify the changed UI renders correctly
- Test interactive elements (forms, buttons, navigation)
- Check responsive behavior if layout was changed

For library/utility changes:
- Write a small test script exercising the changed functions
- Verify edge cases mentioned in the change description

### Step 5: Report

Provide a clear verification report:

```
VERIFICATION REPORT
==================
Build:     PASS/FAIL (details if fail)
Tests:     PASS/FAIL (X passed, Y failed — list failures)
Manual:    PASS/FAIL (what was tested and results)
Overall:   PASS/FAIL
```

If any step fails:
1. Diagnose the root cause
2. Fix the issue if it's clearly related to the recent changes
3. Re-run verification after the fix
4. Report what was fixed

## Important Rules

- Do NOT claim "all tests pass" without actually running them
- Do NOT skip verification steps — if you can't run something, say so explicitly
- Do NOT fix unrelated issues during verification — only fix what the changes broke
- Report outcomes faithfully — failures are valuable information
- If the project has no test infrastructure, note this and focus on build + manual testing
"#;
