use super::{Tool, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct NotebookEditTool;

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &'static str {
        "NotebookEdit"
    }

    fn description(&self) -> &'static str {
        "Edit Jupyter notebook (.ipynb) cells: add, edit, or delete cells.\n\
         \n\
         Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file)\n\
         with new source. Jupyter notebooks are interactive documents that combine code, text,\n\
         and visualizations, commonly used for data analysis and scientific computing.\n\
         \n\
         The notebook_path parameter must be an absolute path, not a relative path.\n\
         The cell_index is 0-indexed.\n\
         \n\
         Commands:\n\
         - add_cell: Insert a new cell at the given index (or append at the end)\n\
         - edit_cell: Replace cell source content at the given index\n\
         - delete_cell: Remove the cell at the given index\n\
         \n\
         For editing other file types, use the Edit or Write tools."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
                },
                "command": {
                    "type": "string",
                    "enum": ["add_cell", "edit_cell", "delete_cell"],
                    "description": "The operation to perform on the notebook"
                },
                "cell_index": {
                    "type": "integer",
                    "description": "The 0-based index of the cell to operate on. For add_cell, the new cell is inserted at this index (omit to append). For edit_cell and delete_cell, this specifies which cell to modify."
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of cell. Required when using add_cell. For edit_cell, changes the cell type if specified."
                },
                "content": {
                    "type": "string",
                    "description": "The source content for the cell. Required for add_cell and edit_cell."
                }
            },
            "required": ["notebook_path", "command"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value, ctx: &ToolContext<'_>) -> ToolResult {
        // Reject writes in read-only mode
        if ctx.read_only {
            return ToolResult::err("NotebookEdit is not available in read-only mode");
        }

        let notebook_path = match input.get("notebook_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing required parameter: notebook_path"),
        };

        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::err("Missing required parameter: command"),
        };

        // Validate command
        if command != "add_cell" && command != "edit_cell" && command != "delete_cell" {
            return ToolResult::err(format!(
                "Invalid command: '{command}'. Must be one of: add_cell, edit_cell, delete_cell"
            ));
        }

        // Validate path is a .ipynb file
        if !notebook_path.ends_with(".ipynb") {
            return ToolResult::err(
                "File must be a Jupyter notebook (.ipynb file). \
                 For editing other file types, use the Edit tool.",
            );
        }

        // Resolve and validate the path
        let full_path = match super::path_utils::resolve_path(notebook_path, ctx.workspace) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Invalid path: {e}")),
        };

        // Read the notebook file
        let file_content = match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if command == "add_cell" {
                    // Create a new notebook with the cell
                    return create_new_notebook(&full_path, &input).await;
                }
                return ToolResult::err(format!("Notebook file not found: {notebook_path}"));
            }
            Err(e) => return ToolResult::err(format!("Failed to read notebook: {e}")),
        };

        // Parse the notebook JSON
        let mut notebook: Value = match serde_json::from_str(&file_content) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(format!("Notebook is not valid JSON: {e}")),
        };

        // Get the cells array
        let cells = match notebook.get_mut("cells").and_then(|v| v.as_array_mut()) {
            Some(c) => c,
            None => return ToolResult::err("Notebook JSON missing 'cells' array"),
        };

        let cell_count = cells.len();

        match command {
            "add_cell" => {
                let cell_type = input
                    .get("cell_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("code");

                if cell_type != "code" && cell_type != "markdown" {
                    return ToolResult::err(format!(
                        "Invalid cell_type: '{cell_type}'. Must be 'code' or 'markdown'"
                    ));
                }

                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let new_cell = make_cell(cell_type, content);

                let index = input
                    .get("cell_index")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);

                match index {
                    Some(idx) if idx > cell_count => {
                        return ToolResult::err(format!(
                            "cell_index {idx} is out of range (notebook has {cell_count} cells, \
                             max insert index is {cell_count})"
                        ));
                    }
                    Some(idx) => cells.insert(idx, new_cell),
                    None => cells.push(new_cell),
                }

                let new_count = cells.len();
                write_notebook(&full_path, &notebook).await?;

                ToolResult::ok(format!(
                    "Added {cell_type} cell to notebook. Total cells: {new_count}"
                ))
            }

            "edit_cell" => {
                let cell_index = match input.get("cell_index").and_then(|v| v.as_u64()) {
                    Some(idx) => idx as usize,
                    None => return ToolResult::err("Missing required parameter: cell_index for edit_cell"),
                };

                if cell_index >= cell_count {
                    return ToolResult::err(format!(
                        "cell_index {cell_index} is out of range (notebook has {cell_count} cells)"
                    ));
                }

                let content = match input.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::err("Missing required parameter: content for edit_cell"),
                };

                // Update the cell source
                let cell = &mut cells[cell_index];

                // Convert source to the line-array format that .ipynb uses
                let source_value = source_to_value(content);
                cell["source"] = source_value;

                // If cell_type is specified, change it
                if let Some(new_type) = input.get("cell_type").and_then(|v| v.as_str()) {
                    if new_type != "code" && new_type != "markdown" {
                        return ToolResult::err(format!(
                            "Invalid cell_type: '{new_type}'. Must be 'code' or 'markdown'"
                        ));
                    }
                    cell["cell_type"] = json!(new_type);
                }

                // Reset execution state for code cells
                if cell.get("cell_type").and_then(|v| v.as_str()) == Some("code") {
                    cell["execution_count"] = Value::Null;
                    cell["outputs"] = json!([]);
                }

                write_notebook(&full_path, &notebook).await?;

                ToolResult::ok(format!(
                    "Edited cell {cell_index}. Total cells: {cell_count}"
                ))
            }

            "delete_cell" => {
                let cell_index = match input.get("cell_index").and_then(|v| v.as_u64()) {
                    Some(idx) => idx as usize,
                    None => {
                        return ToolResult::err(
                            "Missing required parameter: cell_index for delete_cell",
                        )
                    }
                };

                if cell_index >= cell_count {
                    return ToolResult::err(format!(
                        "cell_index {cell_index} is out of range (notebook has {cell_count} cells)"
                    ));
                }

                cells.remove(cell_index);
                let new_count = cells.len();

                write_notebook(&full_path, &notebook).await?;

                ToolResult::ok(format!(
                    "Deleted cell {cell_index}. Total cells: {new_count}"
                ))
            }

            _ => ToolResult::err(format!("Unknown command: {command}")),
        }
    }
}

/// Build a new notebook cell value.
fn make_cell(cell_type: &str, content: &str) -> Value {
    let source = source_to_value(content);

    if cell_type == "code" {
        json!({
            "cell_type": "code",
            "source": source,
            "metadata": {},
            "execution_count": null,
            "outputs": []
        })
    } else {
        json!({
            "cell_type": "markdown",
            "source": source,
            "metadata": {}
        })
    }
}

/// Convert a content string to the ipynb source format (array of lines with newlines preserved).
fn source_to_value(content: &str) -> Value {
    if content.is_empty() {
        return json!([]);
    }

    let lines: Vec<&str> = content.split('\n').collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 {
            // All lines except the last get a trailing newline
            result.push(format!("{line}\n"));
        } else if !line.is_empty() {
            // Last line: only include if non-empty
            result.push(line.to_string());
        }
    }

    json!(result)
}

/// Create a brand new notebook with a single cell.
async fn create_new_notebook(
    full_path: &std::path::Path,
    input: &Value,
) -> ToolResult {
    let cell_type = input
        .get("cell_type")
        .and_then(|v| v.as_str())
        .unwrap_or("code");

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let cell = make_cell(cell_type, content);

    let notebook = json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {
            "language_info": {
                "name": "python"
            }
        },
        "cells": [cell]
    });

    match write_notebook(full_path, &notebook).await {
        Ok(()) => ToolResult::ok(format!(
            "Created new notebook with 1 {cell_type} cell. Total cells: 1"
        )),
        Err(result) => result,
    }
}

/// Write the notebook JSON to disk with standard ipynb indentation.
async fn write_notebook(path: &std::path::Path, notebook: &Value) -> Result<(), ToolResult> {
    let formatted = serde_json::to_string_pretty(notebook)
        .map_err(|e| ToolResult::err(format!("Failed to serialize notebook: {e}")))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ToolResult::err(format!("Failed to create directory: {e}")))?;
    }

    tokio::fs::write(path, &formatted)
        .await
        .map_err(|e| ToolResult::err(format!("Failed to write notebook: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_notebook_edit() {
        let tool = NotebookEditTool;
        assert_eq!(tool.name(), "NotebookEdit");
    }

    #[test]
    fn is_never_read_only() {
        let tool = NotebookEditTool;
        assert!(!tool.is_read_only(&json!({})));
        assert!(!tool.is_read_only(&json!({"command": "add_cell"})));
    }

    #[test]
    fn schema_has_required_fields() {
        let tool = NotebookEditTool;
        let schema = tool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("notebook_path")));
        assert!(required.contains(&json!("command")));
    }

    #[test]
    fn schema_command_enum() {
        let tool = NotebookEditTool;
        let schema = tool.input_schema();
        let command_enum = schema["properties"]["command"]["enum"]
            .as_array()
            .unwrap();
        assert!(command_enum.contains(&json!("add_cell")));
        assert!(command_enum.contains(&json!("edit_cell")));
        assert!(command_enum.contains(&json!("delete_cell")));
    }

    #[test]
    fn source_to_value_empty() {
        let val = source_to_value("");
        assert_eq!(val, json!([]));
    }

    #[test]
    fn source_to_value_single_line() {
        let val = source_to_value("print('hello')");
        assert_eq!(val, json!(["print('hello')"]));
    }

    #[test]
    fn source_to_value_multi_line() {
        let val = source_to_value("line1\nline2\nline3");
        assert_eq!(val, json!(["line1\n", "line2\n", "line3"]));
    }

    #[test]
    fn make_code_cell() {
        let cell = make_cell("code", "x = 1");
        assert_eq!(cell["cell_type"], "code");
        assert_eq!(cell["execution_count"], Value::Null);
        assert_eq!(cell["outputs"], json!([]));
    }

    #[test]
    fn make_markdown_cell() {
        let cell = make_cell("markdown", "# Title");
        assert_eq!(cell["cell_type"], "markdown");
        assert!(cell.get("execution_count").is_none());
        assert!(cell.get("outputs").is_none());
    }
}
