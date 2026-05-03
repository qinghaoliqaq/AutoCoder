//! Skill discovery — walks compiled-in builtins plus Warp/Claude/Codex skill
//! directories on disk, then deduplicates by name with provider-rank priority.
//!
//! Search order (highest priority first):
//!   1. `<workspace>/.agents/skills/<name>/SKILL.md`           (Project)
//!   2. `~/.config/ai-dev-hub/skills/<name>/SKILL.md`          (User)
//!   3. `~/.claude/skills/<name>/SKILL.md`                     (Claude)
//!   4. `~/.codex/skills/<name>/SKILL.md`                      (Codex)
//!   5. Builtins compiled into the binary                      (Builtin)
//!
//! Skills with the same `name` resolve to the entry with the lowest
//! `SkillProvider::rank()` — i.e. project-level skills override user-level
//! which override Claude/Codex which override builtins.

use super::parsed_skill::{ParsedSkill, SkillProvider, SkillScope};
use std::path::Path;

// ── Compiled-in builtins ─────────────────────────────────────────────────────

/// Hand-listed builtin SKILL.md sources, embedded at compile time.
/// The fallback name is the parent directory; if the SKILL.md frontmatter
/// declares its own `name`, that wins.
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    ("simplify", include_str!("simplify/SKILL.md")),
    ("verify", include_str!("verify/SKILL.md")),
    ("frontend-dev", include_str!("frontend_dev/SKILL.md")),
    ("fullstack-dev", include_str!("fullstack_dev/SKILL.md")),
    ("ui-design-system", include_str!("ui_design_system/SKILL.md")),
    ("write-tech-spec", include_str!("write_tech_spec/SKILL.md")),
    ("implement-specs", include_str!("implement_specs/SKILL.md")),
    (
        "spec-driven-implementation",
        include_str!("spec_driven_implementation/SKILL.md"),
    ),
];

/// Load every embedded builtin skill. Panics on parse error — these are
/// shipped with the binary and must always parse, so a bad SKILL.md is a
/// build-time bug, not a runtime condition.
pub fn load_builtins() -> Vec<ParsedSkill> {
    BUILTIN_SKILLS
        .iter()
        .map(|(slug, raw)| {
            ParsedSkill::from_embedded(raw, slug)
                .unwrap_or_else(|e| panic!("builtin skill `{slug}` failed to parse: {e}"))
        })
        .collect()
}

// ── Filesystem discovery ─────────────────────────────────────────────────────

/// Discover all skills available from the given workspace, including builtins,
/// user-level skills, and Claude/Codex skill directories. Filesystem read
/// errors are logged and skipped — discovery is best-effort.
pub fn discover_all(workspace: Option<&Path>) -> Vec<ParsedSkill> {
    let mut skills = Vec::new();

    if let Some(ws) = workspace {
        skills.extend(scan_dir(
            &ws.join(".agents").join("skills"),
            SkillProvider::Project,
            SkillScope::Project,
        ));
    }

    let home = dirs::home_dir();

    if let Some(cfg) = dirs::config_dir() {
        skills.extend(scan_dir(
            &cfg.join("ai-dev-hub").join("skills"),
            SkillProvider::User,
            SkillScope::User,
        ));
    }

    if let Some(h) = home.as_ref() {
        skills.extend(scan_dir(
            &h.join(".claude").join("skills"),
            SkillProvider::Claude,
            SkillScope::User,
        ));
        skills.extend(scan_dir(
            &h.join(".codex").join("skills"),
            SkillProvider::Codex,
            SkillScope::User,
        ));
    }

    skills.extend(load_builtins());
    skills
}

/// Scan a directory for `<name>/SKILL.md` entries. Missing directories are
/// silently skipped (the conventional case — most users won't have one).
fn scan_dir(root: &Path, provider: SkillProvider, scope: SkillScope) -> Vec<ParsedSkill> {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::debug!("skill scan {}: {e}", root.display());
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        match ParsedSkill::from_path(&skill_md, provider, scope) {
            Ok(skill) => out.push(skill),
            Err(e) => tracing::warn!("skipping skill at {}: {e}", skill_md.display()),
        }
    }
    out
}

/// Deduplicate a flat skill list by `name`, keeping the entry with the lowest
/// (= highest-priority) provider rank. Stable: ties keep insertion order.
pub fn dedupe_by_priority(mut skills: Vec<ParsedSkill>) -> Vec<ParsedSkill> {
    use std::collections::HashMap;
    // Pick winner per name.
    let mut winner_idx: HashMap<String, usize> = HashMap::new();
    for (i, s) in skills.iter().enumerate() {
        winner_idx
            .entry(s.name.clone())
            .and_modify(|prev| {
                if s.provider.rank() < skills[*prev].provider.rank() {
                    *prev = i;
                }
            })
            .or_insert(i);
    }
    // Materialize in insertion order, dropping non-winners.
    let mut keep = vec![false; skills.len()];
    for &i in winner_idx.values() {
        keep[i] = true;
    }
    let mut idx = 0;
    skills.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn loads_all_builtins() {
        let skills = load_builtins();
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        for expected in [
            "simplify",
            "verify",
            "frontend-dev",
            "fullstack-dev",
            "ui-design-system",
            "write-tech-spec",
            "implement-specs",
            "spec-driven-implementation",
        ] {
            assert!(names.contains(&expected), "missing builtin: {expected}");
        }
        assert_eq!(skills.len(), 8);
    }

    #[test]
    fn builtins_carry_provider_and_descriptions() {
        for s in load_builtins() {
            assert_eq!(s.provider, SkillProvider::Builtin);
            assert!(!s.description.is_empty(), "{}: empty description", s.name);
            assert!(!s.content.is_empty(), "{}: empty content", s.name);
        }
    }

    #[test]
    fn project_scan_picks_up_skills() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".agents").join("skills").join("my-skill");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Custom skill.\n---\n\n# Body\n",
        )
        .unwrap();

        let skills = scan_dir(
            &tmp.path().join(".agents").join("skills"),
            SkillProvider::Project,
            SkillScope::Project,
        );
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].provider, SkillProvider::Project);
    }

    #[test]
    fn missing_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let skills = scan_dir(
            &tmp.path().join("does-not-exist"),
            SkillProvider::Project,
            SkillScope::Project,
        );
        assert!(skills.is_empty());
    }

    #[test]
    fn malformed_skill_is_skipped_not_fatal() {
        let tmp = TempDir::new().unwrap();
        let bad = tmp.path().join("bad");
        let good = tmp.path().join("good");
        fs::create_dir_all(&bad).unwrap();
        fs::create_dir_all(&good).unwrap();
        fs::write(bad.join("SKILL.md"), "no frontmatter here").unwrap();
        fs::write(
            good.join("SKILL.md"),
            "---\nname: good\ndescription: Works.\n---\nbody",
        )
        .unwrap();

        let skills = scan_dir(tmp.path(), SkillProvider::Project, SkillScope::Project);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "good");
    }

    #[test]
    fn dedupe_prefers_project_over_builtin() {
        // Simulate a project skill named "simplify" that should override the builtin.
        let raw = "---\nname: simplify\ndescription: Project override.\n---\nbody";
        let project = ParsedSkill::parse(
            raw,
            SkillProvider::Project,
            SkillScope::Project,
            super::super::parsed_skill::SkillSource::Embedded,
            None,
        )
        .unwrap();

        let mut skills = vec![project];
        skills.extend(load_builtins());

        let deduped = dedupe_by_priority(skills);
        let simplify = deduped.iter().find(|s| s.name == "simplify").unwrap();
        assert_eq!(simplify.provider, SkillProvider::Project);
        assert_eq!(simplify.description, "Project override.");
        // All builtin names still resolvable after the project override.
        for n in [
            "simplify",
            "verify",
            "frontend-dev",
            "fullstack-dev",
            "ui-design-system",
            "write-tech-spec",
            "implement-specs",
            "spec-driven-implementation",
        ] {
            assert!(deduped.iter().any(|s| s.name == n), "missing {n}");
        }
    }

    #[test]
    fn dedupe_is_stable_across_priorities() {
        let make = |name: &str, provider| -> ParsedSkill {
            ParsedSkill::parse(
                &format!("---\nname: {name}\ndescription: x.\n---\nbody"),
                provider,
                SkillScope::User,
                super::super::parsed_skill::SkillSource::Embedded,
                None,
            )
            .unwrap()
        };
        let skills = vec![
            make("a", SkillProvider::Builtin),
            make("a", SkillProvider::Claude),
            make("a", SkillProvider::Project),
            make("b", SkillProvider::Builtin),
        ];
        let out = dedupe_by_priority(skills);
        assert_eq!(out.len(), 2);
        let a = out.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(a.provider, SkillProvider::Project);
    }
}
