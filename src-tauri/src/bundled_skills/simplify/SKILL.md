---
name: simplify
label: Simplify
category: review
description: Review recent code changes for reuse, quality, and efficiency. Use
  when the user asks for a code-quality pass, requests "/simplify", or asks to
  audit recent changes for duplication or technical debt.
---

# Simplify — Code Review for Reuse, Quality, and Efficiency

Review the recent code changes across three dimensions simultaneously.
For each dimension, analyze the changed files and provide actionable findings.

## Review Dimension 1: Code Reuse

Search the codebase for existing utilities, helpers, and patterns that the
changed code could leverage instead of reimplementing:

- Look for existing functions that do the same thing (or nearly the same)
- Identify copy-pasted logic that could use a shared helper
- Find framework/library utilities being reimplemented by hand
- Check if there are existing types, constants, or enums that should be reused

For each finding, provide:
- The duplicated code location (file:line)
- The existing utility that should be used instead
- A suggested fix

## Review Dimension 2: Code Quality

Review the changes for patterns that indicate technical debt or fragility:

- Overly complex conditionals that could be simplified
- Magic numbers or strings that should be named constants
- Functions doing too many things (violating single responsibility)
- Error handling that swallows or hides failures
- Naming that doesn't clearly communicate intent
- Dead code paths or unreachable branches
- Missing edge case handling at system boundaries

For each finding, provide:
- The problematic code location
- Why it's a problem
- A suggested improvement

## Review Dimension 3: Efficiency

Review for unnecessary computational work or architectural inefficiency:

- N+1 query patterns or redundant data fetches
- Unnecessary allocations in hot paths
- Blocking operations that could be async
- Missing caching for expensive repeated computations
- Unnecessary serialization/deserialization cycles
- Large data structures being cloned when references would suffice
- Sequential operations that could run concurrently

For each finding, provide:
- The inefficient code location
- The performance impact (estimated)
- A suggested optimization

## Output Format

Organize findings by dimension. For each dimension:
1. List findings from most to least impactful
2. Include file paths and line numbers
3. Show concrete code suggestions (not just descriptions)
4. Skip trivial issues — focus on changes that meaningfully improve the code

If a dimension has no significant findings, say so briefly and move on.

## Scope

Only review files that were recently changed. Do not review the entire codebase.
Use Grep and Glob to find related code for the reuse analysis.
Read changed files to understand the full context before commenting.
