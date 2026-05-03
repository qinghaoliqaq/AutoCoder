//! ParsedSkill — Warp/Claude/Codex-compatible skill format.
//!
//! A skill is a markdown document with YAML frontmatter:
//!
//! ```text
//! ---
//! name: simplify
//! description: Review code for reuse, quality, and efficiency. Use when the
//!   user asks for a code-quality pass on recent changes.
//! ---
//!
//! # Skill body in markdown...
//! ```
//!
//! Required frontmatter keys: `name`, `description`.
//! Optional: `category`, `label`.
//!
//! The frontmatter parser is intentionally minimal — it handles the scalar
//! string subset of YAML we actually use, without pulling in a full YAML crate.

use std::path::{Path, PathBuf};

/// Where a skill came from. Used for priority ranking when resolving
/// duplicate names: lower-numbered providers win.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillProvider {
    /// Compiled into the binary via `include_str!`.
    Builtin,
    /// `<workspace>/.agents/skills/<name>/SKILL.md` — repo-scoped, highest priority.
    Project,
    /// User-level AutoCoder skills: `~/.config/ai-dev-hub/skills/<name>/SKILL.md`.
    User,
    /// Claude Code skill directories: `~/.claude/skills/<name>/SKILL.md`.
    Claude,
    /// Codex skill directories: `~/.codex/skills/<name>/SKILL.md`.
    Codex,
}

impl SkillProvider {
    /// Lower rank = higher priority when resolving duplicate names.
    /// Project skills override everything; builtins are the fallback.
    pub fn rank(self) -> u8 {
        match self {
            Self::Project => 0,
            Self::User => 1,
            Self::Claude => 2,
            Self::Codex => 3,
            Self::Builtin => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Project => "project",
            Self::User => "user",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// User vs. project scope — mostly for UI grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillScope {
    User,
    Project,
}

/// Where the skill body was loaded from. `Embedded` means the markdown was
/// compiled into the binary; `Path` means it was read from disk at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillSource {
    Embedded,
    Path(PathBuf),
}

/// A loaded, validated skill ready for the model to invoke.
///
/// `category`, `scope`, and `source` are populated by the loader for future
/// surfaces (UI grouping, "where did this skill come from?" diagnostics). They
/// are not consumed by today's call sites, so we mark them `allow(dead_code)`
/// rather than dropping them — keeping the metadata at parse time avoids
/// re-walking the filesystem when those features land.
#[derive(Clone, Debug)]
pub struct ParsedSkill {
    /// Kebab-case identifier (e.g. `simplify`, `frontend-dev`).
    pub name: String,
    /// Human-friendly label. Falls back to a title-cased `name` if absent.
    pub label: String,
    /// One-line action-verb-led trigger description. ≤512 chars by convention
    /// — it's what the model sees when picking a skill.
    pub description: String,
    #[allow(dead_code)]
    pub category: Option<String>,
    /// Markdown body without the frontmatter block.
    pub content: String,
    pub provider: SkillProvider,
    #[allow(dead_code)]
    pub scope: SkillScope,
    #[allow(dead_code)]
    pub source: SkillSource,
    /// Names of skills cross-referenced under the document's
    /// `## Related Skills` heading. The SkillTool resolves these against
    /// the live registry when rendering its tool output, surfacing them
    /// as discoverable next-step `/<name>` slash commands. Order
    /// preserved from the markdown source; duplicates removed.
    pub related: Vec<String>,
}

impl ParsedSkill {
    /// Parse a SKILL.md document. Frontmatter is required.
    pub fn parse(
        raw: &str,
        provider: SkillProvider,
        scope: SkillScope,
        source: SkillSource,
        fallback_name: Option<&str>,
    ) -> Result<Self, String> {
        let (frontmatter, body) = split_frontmatter(raw)
            .ok_or_else(|| "missing YAML frontmatter (---\\n...\\n---)".to_string())?;
        let fields = parse_frontmatter(frontmatter)?;

        let name = fields
            .get("name")
            .cloned()
            .or_else(|| fallback_name.map(str::to_string))
            .ok_or_else(|| "frontmatter missing required `name` field".to_string())?;
        validate_name(&name)?;

        let description = fields
            .get("description")
            .cloned()
            .ok_or_else(|| "frontmatter missing required `description` field".to_string())?;
        if description.is_empty() {
            return Err("`description` must not be empty".to_string());
        }

        let label = fields
            .get("label")
            .cloned()
            .unwrap_or_else(|| title_case(&name));
        let category = fields.get("category").cloned();

        let body_str = body.trim_start_matches('\n').to_string();
        let related = extract_related_skills(&body_str);

        Ok(Self {
            name,
            label,
            description,
            category,
            content: body_str,
            provider,
            scope,
            source,
            related,
        })
    }

    /// Convenience for embedded SKILL.md content compiled in via include_str!.
    pub fn from_embedded(raw: &str, fallback_name: &str) -> Result<Self, String> {
        Self::parse(
            raw,
            SkillProvider::Builtin,
            SkillScope::User,
            SkillSource::Embedded,
            Some(fallback_name),
        )
    }

    /// Load and parse a SKILL.md file from disk.
    pub fn from_path(
        path: &Path,
        provider: SkillProvider,
        scope: SkillScope,
    ) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        // Fallback name = parent directory if frontmatter omits it.
        let fallback = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(str::to_string);
        Self::parse(
            &raw,
            provider,
            scope,
            SkillSource::Path(path.to_path_buf()),
            fallback.as_deref(),
        )
    }
}

// ── Frontmatter parsing ──────────────────────────────────────────────────────

/// Split a SKILL.md document into (frontmatter_body, markdown_body).
/// Returns `None` if the document doesn't start with `---\n`.
fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    // Tolerate UTF-8 BOM and leading blank lines.
    let trimmed = raw.trim_start_matches('\u{feff}').trim_start_matches(['\r', '\n']);
    let after_open = trimmed.strip_prefix("---\n").or_else(|| trimmed.strip_prefix("---\r\n"))?;
    // Closing delimiter must be on its own line.
    let close_idx_lf = after_open.find("\n---\n");
    let close_idx_crlf = after_open.find("\r\n---\r\n");
    let (close, skip) = match (close_idx_lf, close_idx_crlf) {
        (Some(a), Some(b)) if a <= b => (a, "\n---\n".len()),
        (_, Some(b)) => (b, "\r\n---\r\n".len()),
        (Some(a), None) => (a, "\n---\n".len()),
        (None, None) => {
            // Allow EOF immediately after closing `---` (no trailing newline).
            let tail = after_open.trim_end();
            return tail.strip_suffix("---").map(|fm| (fm.trim_end(), ""));
        }
    };
    let frontmatter = &after_open[..close];
    let body = &after_open[close + skip..];
    Some((frontmatter, body))
}

/// Parse the limited subset of YAML we support: `key: value` lines, with
/// optional double-quoted values and folded multi-line values via leading
/// whitespace continuation. Comments (`#`) and blank lines are skipped.
fn parse_frontmatter(text: &str) -> Result<std::collections::HashMap<String, String>, String> {
    use std::collections::HashMap;
    let mut out: HashMap<String, String> = HashMap::new();
    let mut last_key: Option<String> = None;

    for (lineno, raw_line) in text.lines().enumerate() {
        let line_num = lineno + 1;
        let line = raw_line.trim_end_matches(['\r', ' ', '\t']);

        if line.is_empty() {
            // Blank line ends any folded continuation.
            last_key = None;
            continue;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }

        // Continuation line: starts with whitespace → fold into previous value.
        if raw_line.starts_with([' ', '\t']) {
            if let Some(key) = last_key.as_ref() {
                if let Some(prev) = out.get_mut(key) {
                    if !prev.is_empty() {
                        prev.push(' ');
                    }
                    prev.push_str(line.trim());
                    continue;
                }
            }
            return Err(format!(
                "line {line_num}: indented continuation with no preceding key"
            ));
        }

        // `key: value` — value may be empty, quoted, or trailing on the next
        // continuation line(s).
        let (key, value) = line.split_once(':').ok_or_else(|| {
            format!("line {line_num}: expected `key: value`, got `{line}`")
        })?;
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(format!("line {line_num}: empty key"));
        }
        let raw_val = value.trim();
        let stripped_val = unquote(raw_val);
        out.insert(key.clone(), stripped_val);
        last_key = Some(key);
    }

    Ok(out)
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 {
        let b = s.as_bytes();
        if (b[0] == b'"' && b[s.len() - 1] == b'"') || (b[0] == b'\'' && b[s.len() - 1] == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("`name` must not be empty".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "`name` must be kebab-case (lowercase, digits, hyphens): got `{name}`"
        ));
    }
    Ok(())
}

fn title_case(name: &str) -> String {
    name.split('-')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Related Skills extraction ────────────────────────────────────────────────
//
// Convention: somewhere in the body, a markdown section like
//
//     ## Related Skills
//     - `implement-specs` — picks up where this leaves off.
//     - `verify` — sanity-check before shipping.
//
// We extract the kebab-case identifiers backticked at the start of each list
// item under the heading. Stops at the next H2/H1 boundary or EOF. Tolerant
// of common formatting variations: `## ` / `### `, `- `/`* `/`1. ` list
// markers, "Related Skills" / "Related skills" / "Related Skill" casing.

/// Find the `## Related Skills` section (if any) and pull out the kebab-
/// case skill names referenced as the first backticked token of each list
/// item. Order preserved; duplicates dropped.
fn extract_related_skills(body: &str) -> Vec<String> {
    let mut lines = body.lines();
    // Phase 1 — locate the heading.
    let mut found = false;
    for line in lines.by_ref() {
        if is_related_skills_heading(line) {
            found = true;
            break;
        }
    }
    if !found {
        return Vec::new();
    }

    // Phase 2 — collect list items until the next heading boundary.
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in lines {
        let trimmed = line.trim_start();
        // Any new heading at H1 / H2 ends the section. Sub-headings (H3+)
        // don't terminate, so a skill description containing a `### Note`
        // wouldn't accidentally cut us off — though list items appearing
        // after a sub-heading are still scanned.
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
            break;
        }
        if let Some(name) = extract_skill_ref_from_list_item(trimmed) {
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
    }
    out
}

fn is_related_skills_heading(line: &str) -> bool {
    let trimmed = line.trim();
    let after_hashes = trimmed.trim_start_matches('#').trim();
    if after_hashes.len() == trimmed.len() {
        return false; // no leading `#`
    }
    let lower = after_hashes.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "related skills" | "related skill" | "see also"
    )
}

/// From a single (already-trimmed-leading) line, extract the first kebab-
/// case identifier wrapped in backticks at the start of a markdown list
/// item. Returns None if the line isn't a list item or has no leading
/// backticked identifier.
fn extract_skill_ref_from_list_item(line: &str) -> Option<String> {
    // Strip the list marker. Accept `- `, `* `, `+ `, or a numeric
    // ordered-list prefix `1. ` / `12. `.
    let after_marker = strip_list_marker(line)?;
    // Must start with a backtick — otherwise this isn't a skill ref line.
    let after_tick = after_marker.strip_prefix('`')?;
    // Identifier is everything up to the closing backtick.
    let close = after_tick.find('`')?;
    let candidate = &after_tick[..close];
    if is_kebab_identifier(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn strip_list_marker(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("- ") {
        return Some(rest);
    }
    if let Some(rest) = line.strip_prefix("* ") {
        return Some(rest);
    }
    if let Some(rest) = line.strip_prefix("+ ") {
        return Some(rest);
    }
    // Ordered list: a run of digits followed by `. `.
    let digits_end = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_end > 0 {
        let after = &line[digits_end..];
        if let Some(rest) = after.strip_prefix(". ") {
            return Some(rest);
        }
    }
    None
}

fn is_kebab_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(raw: &str) -> ParsedSkill {
        ParsedSkill::from_embedded(raw, "fallback").expect("parse")
    }

    #[test]
    fn parses_minimal_skill() {
        let raw = "---\nname: simplify\ndescription: Review code.\n---\n\n# Body\n";
        let skill = parse_ok(raw);
        assert_eq!(skill.name, "simplify");
        assert_eq!(skill.description, "Review code.");
        assert_eq!(skill.label, "Simplify");
        assert!(skill.content.starts_with("# Body"));
        assert_eq!(skill.provider, SkillProvider::Builtin);
    }

    #[test]
    fn parses_quoted_values() {
        let raw = "---\nname: \"verify\"\ndescription: 'Verify changes.'\n---\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.name, "verify");
        assert_eq!(skill.description, "Verify changes.");
    }

    #[test]
    fn parses_optional_label_and_category() {
        let raw = "---\nname: simplify\nlabel: Simplify Pass\ncategory: review\n\
                   description: Review code.\n---\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.label, "Simplify Pass");
        assert_eq!(skill.category.as_deref(), Some("review"));
    }

    #[test]
    fn folds_multiline_description() {
        let raw = "---\nname: simplify\ndescription: Review code\n  for reuse\n  \
                   and quality.\n---\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.description, "Review code for reuse and quality.");
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let err = ParsedSkill::from_embedded("# just markdown\n", "x").unwrap_err();
        assert!(err.contains("frontmatter"));
    }

    #[test]
    fn rejects_missing_name_with_no_fallback() {
        let raw = "---\ndescription: x\n---\nbody";
        let err = ParsedSkill::parse(
            raw,
            SkillProvider::Builtin,
            SkillScope::User,
            SkillSource::Embedded,
            None,
        )
        .unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn uses_fallback_name_when_frontmatter_omits_it() {
        let raw = "---\ndescription: x\n---\nbody";
        let skill = ParsedSkill::from_embedded(raw, "my-skill").unwrap();
        assert_eq!(skill.name, "my-skill");
    }

    #[test]
    fn rejects_missing_description() {
        let raw = "---\nname: x\n---\nbody";
        let err = ParsedSkill::from_embedded(raw, "x").unwrap_err();
        assert!(err.contains("description"));
    }

    #[test]
    fn rejects_non_kebab_name() {
        let raw = "---\nname: My_Skill\ndescription: x\n---\nbody";
        let err = ParsedSkill::from_embedded(raw, "fallback").unwrap_err();
        assert!(err.contains("kebab"));
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let raw = "---\n# leading comment\n\nname: simplify\n\n# mid comment\n\
                   description: Review code.\n---\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.name, "simplify");
        assert_eq!(skill.description, "Review code.");
    }

    #[test]
    fn provider_rank_ordering() {
        assert!(SkillProvider::Project.rank() < SkillProvider::User.rank());
        assert!(SkillProvider::User.rank() < SkillProvider::Claude.rank());
        assert!(SkillProvider::Claude.rank() < SkillProvider::Codex.rank());
        assert!(SkillProvider::Codex.rank() < SkillProvider::Builtin.rank());
    }

    #[test]
    fn handles_crlf_line_endings() {
        let raw = "---\r\nname: simplify\r\ndescription: Review.\r\n---\r\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.name, "simplify");
        assert_eq!(skill.description, "Review.");
    }

    #[test]
    fn tolerates_utf8_bom() {
        let raw = "\u{feff}---\nname: simplify\ndescription: Review.\n---\nbody";
        let skill = parse_ok(raw);
        assert_eq!(skill.name, "simplify");
    }

    // ── Related Skills extraction ─────────────────────────────────────────

    fn parse_with_body(body: &str) -> ParsedSkill {
        let raw = format!("---\nname: x\ndescription: x.\n---\n{body}");
        parse_ok(&raw)
    }

    #[test]
    fn related_empty_when_no_section() {
        let skill = parse_with_body("# Body\n\nSome content with no Related Skills heading.");
        assert!(skill.related.is_empty());
    }

    #[test]
    fn extracts_basic_related_skills() {
        let body = "\
## Related Skills

- `implement-specs` — picks up after spec is approved.
- `verify` — final sanity sweep.
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["implement-specs", "verify"]);
    }

    #[test]
    fn related_section_terminates_at_next_h2() {
        let body = "\
## Related Skills

- `verify`

## Anti-Patterns

- `should-not-appear` — different section.
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify"]);
    }

    #[test]
    fn related_h3_heading_works() {
        let body = "\
### Related Skills

- `simplify`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["simplify"]);
    }

    #[test]
    fn related_accepts_see_also_alias() {
        let body = "\
## See Also

- `verify`
- `simplify`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify", "simplify"]);
    }

    #[test]
    fn related_handles_star_and_plus_and_ordered_lists() {
        let body = "\
## Related Skills

* `verify`
+ `simplify`
1. `frontend-dev`
";
        let skill = parse_with_body(body);
        assert_eq!(
            skill.related,
            vec!["verify", "simplify", "frontend-dev"]
        );
    }

    #[test]
    fn related_dedupes_repeats() {
        let body = "\
## Related Skills

- `verify`
- `verify` — listed twice for some reason
- `simplify`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify", "simplify"]);
    }

    #[test]
    fn related_skips_non_kebab_or_uppercase() {
        let body = "\
## Related Skills

- `My_Skill` — not kebab
- `valid-one` — kept
- `Another Skill` — has space
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["valid-one"]);
    }

    #[test]
    fn related_ignores_list_items_without_leading_backtick() {
        let body = "\
## Related Skills

- See the docs — no skill ref here
- `verify` — this one counts
- the `verify` skill — backtick not at start, skipped
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify"]);
    }

    #[test]
    fn related_case_insensitive_heading() {
        let body = "\
## related skills

- `verify`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify"]);
    }

    #[test]
    fn related_terminates_at_h1() {
        let body = "\
## Related Skills

- `verify`

# Different Top-Level

- `out-of-section`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify"]);
    }

    #[test]
    fn related_tolerates_blank_lines_between_items() {
        let body = "\
## Related Skills

- `verify`


- `simplify`
";
        let skill = parse_with_body(body);
        assert_eq!(skill.related, vec!["verify", "simplify"]);
    }
}
