use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(crate) const PLAN_GRAPH_JSON: &str = ".ai-dev-hub/PLAN_GRAPH.json";
pub(crate) const PLAN_ACCEPTANCE_JSON: &str = ".ai-dev-hub/PLAN_ACCEPTANCE.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanGraph {
    pub version: u32,
    pub project_name: String,
    pub project_goal: String,
    pub subtasks: Vec<PlanSubtask>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanSubtask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: SubtaskCategory,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default = "default_true")]
    pub can_run_in_parallel: bool,
    #[serde(default)]
    pub suggested_skill: Option<SuggestedSkill>,
    #[serde(default)]
    pub expected_touch: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanAcceptance {
    pub version: u32,
    #[serde(default)]
    pub project_acceptance: Vec<String>,
    pub subtasks: Vec<SubtaskAcceptance>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubtaskAcceptance {
    pub subtask_id: String,
    #[serde(default)]
    pub must_have: Vec<String>,
    #[serde(default)]
    pub must_not: Vec<String>,
    #[serde(default)]
    pub evidence_required: Vec<String>,
    #[serde(default)]
    pub qa_focus: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubtaskCategory {
    Frontend,
    Backend,
    Fullstack,
    Infra,
    Docs,
    /// Fallback for any category the LLM invents that we don't recognize.
    /// Without this, an unexpected value like "testing" or "database" would
    /// crash the entire plan parsing phase.
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SuggestedSkill {
    #[serde(rename = "frontend-dev")]
    FrontendDev,
    #[serde(rename = "fullstack-dev")]
    FullstackDev,
    #[serde(rename = "ui-design-system")]
    UiDesignSystem,
    /// Fallback for any skill name the LLM invents that we don't recognize.
    #[serde(other)]
    Other,
}

fn default_true() -> bool {
    true
}

pub(crate) fn parse_plan_graph(text: &str) -> Result<PlanGraph, String> {
    let graph = serde_json::from_str::<PlanGraph>(text)
        .map_err(|e| format!("Cannot parse {PLAN_GRAPH_JSON}: {e}"))?;
    validate_plan_graph(&graph)?;
    Ok(graph)
}

pub(crate) fn parse_plan_acceptance(text: &str) -> Result<PlanAcceptance, String> {
    let acceptance = serde_json::from_str::<PlanAcceptance>(text)
        .map_err(|e| format!("Cannot parse {PLAN_ACCEPTANCE_JSON}: {e}"))?;
    validate_plan_acceptance(&acceptance)?;
    Ok(acceptance)
}

pub(crate) fn validate_acceptance_matches_graph(
    graph: &PlanGraph,
    acceptance: &PlanAcceptance,
) -> Result<(), String> {
    let graph_ids: HashSet<&str> = graph
        .subtasks
        .iter()
        .map(|subtask| subtask.id.as_str())
        .collect();
    let acceptance_ids: HashSet<&str> = acceptance
        .subtasks
        .iter()
        .map(|subtask| subtask.subtask_id.as_str())
        .collect();

    for subtask in &acceptance.subtasks {
        if !graph_ids.contains(subtask.subtask_id.as_str()) {
            return Err(format!(
                "{PLAN_ACCEPTANCE_JSON} references unknown subtask id: {}",
                subtask.subtask_id
            ));
        }
    }

    for subtask in &graph.subtasks {
        if !acceptance_ids.contains(subtask.id.as_str()) {
            return Err(format!(
                "{PLAN_ACCEPTANCE_JSON} is missing acceptance criteria for subtask {}",
                subtask.id
            ));
        }
    }

    Ok(())
}

pub(crate) fn read_plan_graph(workspace: &str) -> Result<Option<PlanGraph>, String> {
    let path = Path::new(workspace).join(PLAN_GRAPH_JSON);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    parse_plan_graph(&text).map(Some)
}

pub(crate) fn read_plan_acceptance(workspace: &str) -> Result<Option<PlanAcceptance>, String> {
    let path = Path::new(workspace).join(PLAN_ACCEPTANCE_JSON);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    parse_plan_acceptance(&text).map(Some)
}

pub(crate) fn read_plan_acceptance_lenient(
    workspace: &str,
) -> (Option<PlanAcceptance>, Option<String>) {
    match read_plan_acceptance(workspace) {
        Ok(acceptance) => (acceptance, None),
        Err(err) => (None, Some(err)),
    }
}

fn validate_plan_graph(graph: &PlanGraph) -> Result<(), String> {
    if graph.version != 1 {
        return Err(format!(
            "{PLAN_GRAPH_JSON} must use version 1, got {}",
            graph.version
        ));
    }
    if graph.project_name.trim().is_empty() {
        return Err(format!("{PLAN_GRAPH_JSON} project_name must not be empty"));
    }
    if graph.project_goal.trim().is_empty() {
        return Err(format!("{PLAN_GRAPH_JSON} project_goal must not be empty"));
    }
    if graph.subtasks.is_empty() {
        return Err(format!(
            "{PLAN_GRAPH_JSON} must contain at least one subtask"
        ));
    }

    let mut ids = HashSet::new();
    for subtask in &graph.subtasks {
        if !is_valid_subtask_id(&subtask.id) {
            return Err(format!(
                "{PLAN_GRAPH_JSON} contains invalid subtask id: {}",
                subtask.id
            ));
        }
        if !ids.insert(subtask.id.clone()) {
            return Err(format!(
                "{PLAN_GRAPH_JSON} contains duplicate subtask id: {}",
                subtask.id
            ));
        }
        if subtask.title.trim().is_empty() {
            return Err(format!(
                "{PLAN_GRAPH_JSON} subtask {} must have a non-empty title",
                subtask.id
            ));
        }
        if subtask.description.trim().is_empty() {
            return Err(format!(
                "{PLAN_GRAPH_JSON} subtask {} must have a non-empty description",
                subtask.id
            ));
        }
        if subtask
            .parallel_group
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(format!(
                "{PLAN_GRAPH_JSON} subtask {} has an empty parallel_group",
                subtask.id
            ));
        }
        if subtask
            .expected_touch
            .iter()
            .any(|item| item.trim().is_empty())
        {
            return Err(format!(
                "{PLAN_GRAPH_JSON} subtask {} has an empty expected_touch entry",
                subtask.id
            ));
        }
    }

    let by_id: HashMap<&str, &PlanSubtask> = graph
        .subtasks
        .iter()
        .map(|subtask| (subtask.id.as_str(), subtask))
        .collect();

    for subtask in &graph.subtasks {
        for dep in &subtask.depends_on {
            if dep == &subtask.id {
                return Err(format!(
                    "{PLAN_GRAPH_JSON} subtask {} cannot depend on itself",
                    subtask.id
                ));
            }
            if !by_id.contains_key(dep.as_str()) {
                return Err(format!(
                    "{PLAN_GRAPH_JSON} subtask {} depends on unknown id {}",
                    subtask.id, dep
                ));
            }
        }
    }

    validate_dependency_cycles(graph)?;
    Ok(())
}

fn validate_plan_acceptance(acceptance: &PlanAcceptance) -> Result<(), String> {
    if acceptance.version != 1 {
        return Err(format!(
            "{PLAN_ACCEPTANCE_JSON} must use version 1, got {}",
            acceptance.version
        ));
    }
    if acceptance.subtasks.is_empty() {
        return Err(format!(
            "{PLAN_ACCEPTANCE_JSON} must contain at least one subtask acceptance entry"
        ));
    }

    let mut ids = HashSet::new();
    for subtask in &acceptance.subtasks {
        if !is_valid_subtask_id(&subtask.subtask_id) {
            return Err(format!(
                "{PLAN_ACCEPTANCE_JSON} contains invalid subtask id: {}",
                subtask.subtask_id
            ));
        }
        if !ids.insert(subtask.subtask_id.clone()) {
            return Err(format!(
                "{PLAN_ACCEPTANCE_JSON} contains duplicate subtask id: {}",
                subtask.subtask_id
            ));
        }
        if subtask.must_have.is_empty()
            && subtask.must_not.is_empty()
            && subtask.evidence_required.is_empty()
            && subtask.qa_focus.is_empty()
        {
            return Err(format!(
                "{PLAN_ACCEPTANCE_JSON} subtask {} must define at least one acceptance item",
                subtask.subtask_id
            ));
        }
    }

    Ok(())
}

fn validate_dependency_cycles(graph: &PlanGraph) -> Result<(), String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Done,
    }

    fn visit<'a>(
        node: &'a str,
        graph: &'a HashMap<&'a str, &'a PlanSubtask>,
        states: &mut HashMap<&'a str, VisitState>,
        stack: &mut Vec<&'a str>,
    ) -> Result<(), String> {
        if states.get(node) == Some(&VisitState::Done) {
            return Ok(());
        }
        if states.get(node) == Some(&VisitState::Visiting) {
            stack.push(node);
            return Err(format!(
                "{PLAN_GRAPH_JSON} contains a dependency cycle: {}",
                stack.join(" -> ")
            ));
        }

        states.insert(node, VisitState::Visiting);
        stack.push(node);

        if let Some(subtask) = graph.get(node) {
            for dep in &subtask.depends_on {
                visit(dep, graph, states, stack)?;
            }
        }

        stack.pop();
        states.insert(node, VisitState::Done);
        Ok(())
    }

    let by_id: HashMap<&str, &PlanSubtask> = graph
        .subtasks
        .iter()
        .map(|subtask| (subtask.id.as_str(), subtask))
        .collect();
    let mut states = HashMap::new();

    for subtask in &graph.subtasks {
        let mut stack = Vec::new();
        visit(subtask.id.as_str(), &by_id, &mut states, &mut stack)?;
    }

    Ok(())
}

fn is_valid_subtask_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ── Plan quality validation (non-blocking warnings) ──────────────────────

/// Analyse the plan graph and acceptance criteria for quality issues.
/// Returns a list of human-readable warnings.  These are advisory — they
/// do not prevent code mode from running but highlight likely problems.
pub(crate) fn validate_plan_quality(graph: &PlanGraph, acceptance: &PlanAcceptance) -> Vec<String> {
    let mut warnings = Vec::new();
    check_overlapping_touch(graph, &mut warnings);
    check_missing_expected_touch(graph, &mut warnings);
    check_sequential_bottleneck(graph, &mut warnings);
    check_thin_acceptance(graph, acceptance, &mut warnings);
    warnings
}

/// Warn if multiple subtasks share expected_touch paths (merge-conflict risk).
fn check_overlapping_touch(graph: &PlanGraph, warnings: &mut Vec<String>) {
    let mut path_owners: HashMap<&str, Vec<&str>> = HashMap::new();
    for subtask in &graph.subtasks {
        for path in &subtask.expected_touch {
            path_owners
                .entry(path.as_str())
                .or_default()
                .push(subtask.id.as_str());
        }
    }
    for (path, owners) in &path_owners {
        if owners.len() > 1 {
            warnings.push(format!(
                "Path '{}' is in expected_touch of multiple subtasks ({}). \
                 This increases merge-conflict risk during parallel execution.",
                path,
                owners.join(", ")
            ));
        }
    }
}

/// Warn if a subtask has no expected_touch — the verifier cannot check file scope.
fn check_missing_expected_touch(graph: &PlanGraph, warnings: &mut Vec<String>) {
    for subtask in &graph.subtasks {
        if subtask.expected_touch.is_empty() {
            warnings.push(format!(
                "Subtask {} ('{}') has no expected_touch paths. \
                 The verifier cannot detect out-of-scope file changes.",
                subtask.id, subtask.title
            ));
        }
    }
}

/// Warn if the longest dependency chain exceeds a threshold.
/// Long sequential chains prevent parallel execution and slow down code mode.
fn check_sequential_bottleneck(graph: &PlanGraph, warnings: &mut Vec<String>) {
    const MAX_CHAIN_LENGTH: usize = 4;
    let by_id: HashMap<&str, &PlanSubtask> =
        graph.subtasks.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut depth_cache: HashMap<&str, usize> = HashMap::new();

    fn chain_depth<'a>(
        id: &'a str,
        by_id: &HashMap<&str, &'a PlanSubtask>,
        cache: &mut HashMap<&'a str, usize>,
        remaining: usize,
    ) -> usize {
        if remaining == 0 {
            return 0; // depth guard against cycles in unvalidated graphs
        }
        if let Some(&cached) = cache.get(id) {
            return cached;
        }
        let depth = by_id
            .get(id)
            .map(|subtask| {
                subtask
                    .depends_on
                    .iter()
                    .map(|dep| 1 + chain_depth(dep, by_id, cache, remaining - 1))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        cache.insert(id, depth);
        depth
    }

    let max_depth = graph
        .subtasks
        .iter()
        .map(|s| {
            chain_depth(
                s.id.as_str(),
                &by_id,
                &mut depth_cache,
                graph.subtasks.len(),
            )
        })
        .max()
        .unwrap_or(0);

    if max_depth >= MAX_CHAIN_LENGTH {
        warnings.push(format!(
            "Longest dependency chain is {} steps deep (threshold: {}). \
             Consider breaking sequential dependencies to enable more parallelism.",
            max_depth, MAX_CHAIN_LENGTH
        ));
    }
}

/// Warn if any subtask's acceptance criteria are thin (empty must_have).
fn check_thin_acceptance(
    graph: &PlanGraph,
    acceptance: &PlanAcceptance,
    warnings: &mut Vec<String>,
) {
    let acceptance_by_id: HashMap<&str, &SubtaskAcceptance> = acceptance
        .subtasks
        .iter()
        .map(|a| (a.subtask_id.as_str(), a))
        .collect();

    for subtask in &graph.subtasks {
        if let Some(acc) = acceptance_by_id.get(subtask.id.as_str()) {
            if acc.must_have.is_empty() {
                warnings.push(format!(
                    "Subtask {} ('{}') has no must_have acceptance criteria. \
                     The reviewer cannot verify required behavior.",
                    subtask.id, subtask.title
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_plan_graph() {
        let graph = parse_plan_graph(
            r#"{
                "version": 1,
                "project_name": "Hiring Hub",
                "project_goal": "Coordinate recruiting workflows",
                "subtasks": [
                    {
                        "id": "F1",
                        "title": "Jobs API",
                        "description": "Create job CRUD endpoints",
                        "category": "backend",
                        "depends_on": [],
                        "parallel_group": "backend-core",
                        "can_run_in_parallel": true,
                        "suggested_skill": "fullstack-dev",
                        "expected_touch": ["backend/jobs"]
                    }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(graph.subtasks.len(), 1);
        assert_eq!(
            graph.subtasks[0].suggested_skill,
            Some(SuggestedSkill::FullstackDev)
        );
    }

    #[test]
    fn reject_unknown_dependency() {
        let err = parse_plan_graph(
            r#"{
                "version": 1,
                "project_name": "Hiring Hub",
                "project_goal": "Coordinate recruiting workflows",
                "subtasks": [
                    {
                        "id": "F1",
                        "title": "Jobs API",
                        "description": "Create job CRUD endpoints",
                        "category": "backend",
                        "depends_on": ["F2"]
                    }
                ]
            }"#,
        )
        .unwrap_err();
        assert!(err.contains("depends on unknown id"));
    }

    #[test]
    fn reject_dependency_cycle() {
        let err = parse_plan_graph(
            r#"{
                "version": 1,
                "project_name": "Hiring Hub",
                "project_goal": "Coordinate recruiting workflows",
                "subtasks": [
                    {
                        "id": "F1",
                        "title": "Jobs API",
                        "description": "Create job CRUD endpoints",
                        "category": "backend",
                        "depends_on": ["P1"]
                    },
                    {
                        "id": "P1",
                        "title": "Jobs Page",
                        "description": "Render jobs page",
                        "category": "frontend",
                        "depends_on": ["F1"]
                    }
                ]
            }"#,
        )
        .unwrap_err();
        assert!(err.contains("dependency cycle"));
    }

    #[test]
    fn parse_valid_acceptance() {
        let acceptance = parse_plan_acceptance(
            r#"{
                "version": 1,
                "project_acceptance": ["Core hiring flow works"],
                "subtasks": [
                    {
                        "subtask_id": "F1",
                        "must_have": ["Create jobs"],
                        "must_not": [],
                        "evidence_required": ["API tests"],
                        "qa_focus": ["Validation rules"]
                    }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(acceptance.subtasks.len(), 1);
    }

    #[test]
    fn reject_acceptance_without_entries() {
        let err = parse_plan_acceptance(
            r#"{
                "version": 1,
                "subtasks": [
                    {
                        "subtask_id": "F1",
                        "must_have": [],
                        "must_not": [],
                        "evidence_required": [],
                        "qa_focus": []
                    }
                ]
            }"#,
        )
        .unwrap_err();
        assert!(err.contains("must define at least one acceptance item"));
    }

    #[test]
    fn acceptance_must_cover_all_graph_subtasks() {
        let graph = parse_plan_graph(
            r#"{
                "version": 1,
                "project_name": "Hiring Hub",
                "project_goal": "Coordinate recruiting workflows",
                "subtasks": [
                    {
                        "id": "F1",
                        "title": "Jobs API",
                        "description": "Create job CRUD endpoints",
                        "category": "backend",
                        "depends_on": []
                    }
                ]
            }"#,
        )
        .unwrap();
        let acceptance = parse_plan_acceptance(
            r#"{
                "version": 1,
                "subtasks": [
                    {
                        "subtask_id": "P1",
                        "must_have": ["Render page"]
                    }
                ]
            }"#,
        )
        .unwrap();
        let err = validate_acceptance_matches_graph(&graph, &acceptance).unwrap_err();
        assert!(err.contains("references unknown subtask id"));
    }

    #[test]
    fn read_plan_acceptance_lenient_ignores_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ai-dev-hub")).unwrap();
        std::fs::write(dir.path().join(PLAN_ACCEPTANCE_JSON), "{ invalid json").unwrap();

        let (acceptance, warning) = read_plan_acceptance_lenient(dir.path().to_str().unwrap());
        assert!(acceptance.is_none());
        assert!(warning.unwrap().contains("Cannot parse"));
    }

    // ── Plan quality tests ───────────────────────────────────────────────

    fn sample_graph(subtasks: Vec<PlanSubtask>) -> PlanGraph {
        PlanGraph {
            version: 1,
            project_name: "Test".to_string(),
            project_goal: "Test".to_string(),
            subtasks,
        }
    }

    fn sample_subtask(id: &str) -> PlanSubtask {
        PlanSubtask {
            id: id.to_string(),
            title: format!("Title {id}"),
            description: "desc".to_string(),
            category: SubtaskCategory::Backend,
            depends_on: Vec::new(),
            parallel_group: None,
            can_run_in_parallel: true,
            suggested_skill: None,
            expected_touch: vec![format!("src/{}", id.to_lowercase())],
        }
    }

    fn sample_acceptance(ids: &[&str]) -> PlanAcceptance {
        PlanAcceptance {
            version: 1,
            project_acceptance: Vec::new(),
            subtasks: ids
                .iter()
                .map(|id| SubtaskAcceptance {
                    subtask_id: id.to_string(),
                    must_have: vec!["something".to_string()],
                    must_not: Vec::new(),
                    evidence_required: Vec::new(),
                    qa_focus: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn quality_warns_on_overlapping_touch() {
        let mut s1 = sample_subtask("F1");
        let mut s2 = sample_subtask("F2");
        s1.expected_touch = vec!["src/shared".to_string()];
        s2.expected_touch = vec!["src/shared".to_string()];
        let graph = sample_graph(vec![s1, s2]);
        let acceptance = sample_acceptance(&["F1", "F2"]);
        let warnings = validate_plan_quality(&graph, &acceptance);
        assert!(warnings.iter().any(|w| w.contains("merge-conflict risk")));
    }

    #[test]
    fn quality_warns_on_missing_expected_touch() {
        let mut s1 = sample_subtask("F1");
        s1.expected_touch.clear();
        let graph = sample_graph(vec![s1]);
        let acceptance = sample_acceptance(&["F1"]);
        let warnings = validate_plan_quality(&graph, &acceptance);
        assert!(warnings
            .iter()
            .any(|w| w.contains("no expected_touch paths")));
    }

    #[test]
    fn quality_warns_on_sequential_bottleneck() {
        let s1 = sample_subtask("F1");
        let mut s2 = sample_subtask("F2");
        let mut s3 = sample_subtask("F3");
        let mut s4 = sample_subtask("F4");
        let mut s5 = sample_subtask("F5");
        s2.depends_on = vec!["F1".to_string()];
        s3.depends_on = vec!["F2".to_string()];
        s4.depends_on = vec!["F3".to_string()];
        s5.depends_on = vec!["F4".to_string()];
        let graph = sample_graph(vec![s1, s2, s3, s4, s5]);
        let acceptance = sample_acceptance(&["F1", "F2", "F3", "F4", "F5"]);
        let warnings = validate_plan_quality(&graph, &acceptance);
        assert!(warnings.iter().any(|w| w.contains("dependency chain")));
    }

    #[test]
    fn quality_warns_on_thin_acceptance() {
        let graph = sample_graph(vec![sample_subtask("F1")]);
        let acceptance = PlanAcceptance {
            version: 1,
            project_acceptance: Vec::new(),
            subtasks: vec![SubtaskAcceptance {
                subtask_id: "F1".to_string(),
                must_have: Vec::new(),
                must_not: Vec::new(),
                evidence_required: vec!["tests".to_string()],
                qa_focus: Vec::new(),
            }],
        };
        let warnings = validate_plan_quality(&graph, &acceptance);
        assert!(warnings
            .iter()
            .any(|w| w.contains("no must_have acceptance")));
    }

    #[test]
    fn quality_clean_plan_has_no_warnings() {
        let graph = sample_graph(vec![sample_subtask("F1"), sample_subtask("F2")]);
        let acceptance = sample_acceptance(&["F1", "F2"]);
        let warnings = validate_plan_quality(&graph, &acceptance);
        assert!(warnings.is_empty());
    }
}
