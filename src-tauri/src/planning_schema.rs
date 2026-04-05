use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(crate) const PLAN_GRAPH_JSON: &str = "PLAN_GRAPH.json";
pub(crate) const PLAN_ACCEPTANCE_JSON: &str = "PLAN_ACCEPTANCE.json";

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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SuggestedSkill {
    #[serde(rename = "frontend-dev")]
    FrontendDev,
    #[serde(rename = "fullstack-dev")]
    FullstackDev,
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
        std::fs::write(dir.path().join(PLAN_ACCEPTANCE_JSON), "{ invalid json").unwrap();

        let (acceptance, warning) = read_plan_acceptance_lenient(dir.path().to_str().unwrap());
        assert!(acceptance.is_none());
        assert!(warning.unwrap().contains("Cannot parse"));
    }
}
