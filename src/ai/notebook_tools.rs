use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};
use tokio_util::sync::CancellationToken;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Jupyter notebook structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub cells: Vec<Cell>,
    pub metadata: Value,
    pub nbformat: u32,
    pub nbformat_minor: u32,
}

/// Cell structure in a notebook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub cell_type: String,
    pub source: CellSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Cell source can be either a string or array of strings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellSource {
    String(String),
    Array(Vec<String>),
}

impl CellSource {
    /// Convert to a single string
    pub fn to_string(&self) -> String {
        match self {
            CellSource::String(s) => s.clone(),
            CellSource::Array(lines) => lines.join(""),
        }
    }
    
    /// Create from a string
    pub fn from_string(s: String) -> Self {
        // Split into lines if contains newlines
        if s.contains('\n') {
            let lines: Vec<String> = s.lines().map(|line| format!("{}\n", line)).collect();
            CellSource::Array(lines)
        } else if s.is_empty() {
            CellSource::Array(vec![])
        } else {
            CellSource::String(s)
        }
    }
}

/// NotebookRead tool - Read Jupyter notebooks
pub struct NotebookReadTool;

#[async_trait]
impl ToolHandler for NotebookReadTool {
    fn description(&self) -> String {
        "Reads a Jupyter notebook (.ipynb file) and returns all of the cells with their outputs. Jupyter notebooks are interactive documents that combine code, text, and visualizations, commonly used for data analysis and scientific computing. The notebook_path parameter must be an absolute path, not a relative path.".to_string()
    }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to read (must be absolute, not relative)"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of a specific cell to read. If not provided, all cells will be read."
                }
            },
            "required": ["notebook_path"],
            "additionalProperties": false
        })
    }
    
    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        let notebook_path = input["notebook_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'notebook_path' field".to_string()))?;
        
        let cell_id = input["cell_id"].as_str();
        
        // Check if file exists
        let path = Path::new(notebook_path);
        if !path.exists() {
            return Err(Error::NotFound(format!("Invalid notebook path")));
        }
        
        // Check if it's a .ipynb file
        if path.extension().and_then(|s| s.to_str()) != Some("ipynb") {
            return Err(Error::InvalidInput("File must be a Jupyter notebook (.ipynb file).".to_string()));
        }
        
        // Read the notebook file
        let content = fs::read_to_string(path)
            .map_err(|e| Error::Io(e))?;
        
        // Parse the JSON
        let notebook: Value = serde_json::from_str(&content)
            .map_err(|e| Error::InvalidInput(format!("Invalid notebook format: {}", e)))?;
        
        // Get language from metadata (matches JavaScript: config8205.metadata.language_info?.name ?? "python")
        let language = notebook["metadata"]["language_info"]["name"]
            .as_str()
            .unwrap_or("python");
        
        let cells = notebook["cells"]
            .as_array()
            .ok_or_else(|| Error::InvalidInput("Invalid notebook format: missing cells array".to_string()))?;
        
        // If cell_id is specified, find that specific cell (matches JavaScript logic)
        if let Some(id) = cell_id {
            // Look for cell with matching ID
            for (i, cell) in cells.iter().enumerate() {
                if let Some(cell_id_val) = cell["id"].as_str() {
                    if cell_id_val == id {
                        return Ok(format_cell_js_style(cell, i, language, true));
                    }
                }
            }
            
            return Err(Error::InvalidInput(format!("Cell with ID \"{}\" not found in notebook", id)));
        }
        
        // Return all cells (matches JavaScript: map each cell)
        let formatted_cells: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| format_cell_js_style(cell, i, language, false))
            .collect();
        
        Ok(formatted_cells.join("\n"))
    }
    
    fn action_description(&self, input: &Value) -> String {
        if let Some(path) = input["notebook_path"].as_str() {
            format!("Read notebook: {}", path)
        } else {
            "Read Jupyter notebook".to_string()
        }
    }
    
    fn permission_details(&self, input: &Value) -> String {
        if let Some(path) = input["notebook_path"].as_str() {
            format!("Read notebook at {}", path)
        } else {
            "Read Jupyter notebook".to_string()
        }
    }
}

/// Parse cell ID in the format "cell-N" to extract index N (matches JavaScript Gu function)
fn parse_cell_id(id: &str) -> Option<usize> {
    // Match the pattern cell-N where N is a number
    if let Some(captures) = id.strip_prefix("cell-") {
        captures.parse::<usize>().ok()
    } else {
        // Try parsing as direct number
        id.parse::<usize>().ok()
    }
}

/// Format a cell matching JavaScript stringDecoder238 function
fn format_cell_js_style(cell: &Value, index: usize, language: &str, is_single_cell: bool) -> String {
    // Get cell ID (matches: input20325.id ?? `cell-${config8199}`)
    let cell_id = cell["id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("cell-{}", index));
    
    // Get cell type
    let cell_type = cell["cell_type"]
        .as_str()
        .unwrap_or("code");
    
    // Get source (matches: Array.isArray(input20325.source) ? input20325.source.join("") : input20325.source)
    let source = if let Some(arr) = cell["source"].as_array() {
        arr.iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("")
    } else if let Some(s) = cell["source"].as_str() {
        s.to_string()
    } else {
        String::new()
    };
    
    // Build cell representation matching JavaScript format
    let mut result = String::new();
    
    // Add cell header with XML-like format (matches stringDecoder239)
    result.push_str(&format!("<cell id=\"{}\">\n", cell_id));
    
    // Add cell type if not code
    if cell_type != "code" {
        result.push_str(&format!("<cell_type>{}</cell_type>\n", cell_type));
    }
    
    // Add language for code cells if not python
    if cell_type == "code" && language != "python" {
        result.push_str(&format!("<language>{}</language>\n", language));
    }
    
    // Add source
    result.push_str(&source);
    if !source.ends_with('\n') {
        result.push('\n');
    }
    
    // Handle outputs for code cells
    if cell_type == "code" {
        if let Some(outputs) = cell["outputs"].as_array() {
            if !outputs.is_empty() && (is_single_cell || serde_json::to_string(&outputs).unwrap_or_default().len() <= 10000) {
                for output in outputs {
                    result.push_str(&format_output_js_style(output));
                }
            } else if !is_single_cell && !outputs.is_empty() {
                // Outputs too large message (matches JavaScript)
                result.push_str(&format!("\nOutputs are too large to include. Use NotebookRead with parameter cell_id={} to read cell outputs\n", cell_id));
            }
        }
    }
    
    // Add execution count if present
    if let Some(count) = cell["execution_count"].as_u64() {
        result.push_str(&format!("\n[Execution count: {}]\n", count));
    }
    
    result.push_str(&format!("</cell id=\"{}\">\n", cell_id));
    
    result
}

/// Format output matching JavaScript func264 function
fn format_output_js_style(output: &Value) -> String {
    let output_type = output["output_type"].as_str().unwrap_or("");
    
    match output_type {
        "stream" => {
            if let Some(text) = output["text"].as_str() {
                format!("\n{}\n", text)
            } else if let Some(arr) = output["text"].as_array() {
                let text = arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                format!("\n{}\n", text)
            } else {
                String::new()
            }
        }
        "execute_result" | "display_data" => {
            let mut result = String::new();
            if let Some(data) = output["data"].as_object() {
                // Text output
                if let Some(text_plain) = data.get("text/plain") {
                    if let Some(s) = text_plain.as_str() {
                        result.push_str(&format!("\n{}\n", s));
                    } else if let Some(arr) = text_plain.as_array() {
                        let text = arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join("");
                        result.push_str(&format!("\n{}\n", text));
                    }
                }
                // Image output (simplified - would need base64 handling for full implementation)
                if data.contains_key("image/png") || data.contains_key("image/jpeg") {
                    result.push_str("\n[Image output]\n");
                }
            }
            result
        }
        "error" => {
            let ename = output["ename"].as_str().unwrap_or("Error");
            let evalue = output["evalue"].as_str().unwrap_or("");
            let traceback = if let Some(arr) = output["traceback"].as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            };
            format!("\n{}: {}\n{}\n", ename, evalue, traceback)
        }
        _ => String::new()
    }
}

/// NotebookEdit tool - Edit Jupyter notebooks
pub struct NotebookEditTool;

#[async_trait]
impl ToolHandler for NotebookEditTool {
    fn description(&self) -> String {
        "Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) with new source. Jupyter notebooks are interactive documents that combine code, text, and visualizations, commonly used for data analysis and scientific computing. The notebook_path parameter must be an absolute path, not a relative path. The cell_number is 0-indexed. Use edit_mode=insert to add a new cell at the index specified by cell_number. Use edit_mode=delete to delete the cell at the index specified by cell_number.".to_string()
    }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. When inserting a new cell, the new cell will be inserted after the cell with this ID, or at the beginning if not specified."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell (code or markdown). If not specified, it defaults to the current cell type. If using edit_mode=insert, this is required."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make (replace, insert, delete). Defaults to replace."
                }
            },
            "required": ["notebook_path", "new_source"],
            "additionalProperties": false
        })
    }
    
    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        let notebook_path = input["notebook_path"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'notebook_path' field".to_string()))?;
        
        let new_source = input["new_source"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'new_source' field".to_string()))?;
        
        let cell_id = input["cell_id"].as_str();
        let cell_type = input["cell_type"].as_str();
        let edit_mode = input["edit_mode"].as_str().unwrap_or("replace");
        
        // Check if file exists
        let path = Path::new(notebook_path);
        if !path.exists() {
            return Err(Error::NotFound(format!("Notebook file does not exist.")));
        }
        
        // Check if it's a .ipynb file
        if path.extension().and_then(|s| s.to_str()) != Some("ipynb") {
            return Err(Error::InvalidInput("File must be a Jupyter notebook (.ipynb file). For editing other file types, use the FileEdit tool.".to_string()));
        }
        
        // Validate edit mode
        if edit_mode != "replace" && edit_mode != "insert" && edit_mode != "delete" {
            return Err(Error::InvalidInput("Edit mode must be replace, insert, or delete.".to_string()));
        }
        
        // Validate insert mode requires cell_type
        if edit_mode == "insert" && cell_type.is_none() {
            return Err(Error::InvalidInput("Cell type is required when using edit_mode=insert.".to_string()));
        }
        
        // Read the notebook file
        let content = fs::read_to_string(path)
            .map_err(|e| Error::Io(e))?;
        
        // Parse the JSON
        let mut notebook: Value = serde_json::from_str(&content)
            .map_err(|_e| Error::InvalidInput(format!("Notebook is not valid JSON.")))?;
        
        // Find cell index matching JavaScript logic
        let mut cell_index = None;
        if let Some(id) = cell_id {
            let cells_arr = notebook["cells"]
                .as_array()
                .ok_or_else(|| Error::InvalidInput("Invalid notebook format: missing cells array".to_string()))?;
            
            // Check if it matches Gu function pattern
            if let Some(idx) = parse_cell_id(id) {
                if idx < cells_arr.len() {
                    cell_index = Some(idx);
                }
            } else {
                // Look for cell with matching ID
                cell_index = cells_arr.iter().position(|cell| {
                    cell["id"].as_str().map(|cid| cid == id).unwrap_or(false)
                });
            }
            
            if cell_index.is_none() && edit_mode != "insert" {
                if let Some(idx) = parse_cell_id(id) {
                    return Err(Error::InvalidInput(format!("Cell with index {} does not exist in notebook.", idx)));
                } else {
                    return Err(Error::InvalidInput(format!("Cell with ID \"{}\" not found in notebook.", id)));
                }
            }
        } else if edit_mode != "insert" {
            return Err(Error::InvalidInput("Cell ID must be specified when not inserting a new cell.".to_string()));
        }
        
        // Get cells array for mutation (do this after reading nbformat values)
        // This needs to be done just before the match to avoid borrow conflicts
        
        // Perform the edit operation matching JavaScript implementation
        let mut actual_edit_mode = edit_mode.to_string();
        
        // Read nbformat values before getting mutable cells (avoids borrow conflict)
        let nbformat = notebook["nbformat"].as_u64().unwrap_or(0);
        let nbformat_minor = notebook["nbformat_minor"].as_u64().unwrap_or(0);
        
        // Now get the mutable cells array
        let cells = notebook["cells"]
            .as_array_mut()
            .ok_or_else(|| Error::InvalidInput("Invalid notebook format: missing cells array".to_string()))?;
        
        match edit_mode {
            "replace" => {
                let idx = cell_index.unwrap_or(0);
                
                // Special case: if replacing at end, switch to insert (matches JS)
                if idx == cells.len() {
                    actual_edit_mode = "insert".to_string();
                    // Insert at end
                    let new_cell = json!({
                        "cell_type": cell_type.unwrap_or("code"),
                        "source": new_source,
                        "metadata": {},
                        "outputs": if cell_type.unwrap_or("code") == "code" { json!([]) } else { Value::Null }
                    });
                    cells.push(new_cell);
                } else {
                    // Replace existing cell
                    let cell = &mut cells[idx];
                    cell["source"] = json!(new_source);
                    
                    // Update cell type if specified
                    if let Some(ct) = cell_type {
                        if ct != cell["cell_type"].as_str().unwrap_or("") {
                            cell["cell_type"] = json!(ct);
                            // Clear outputs and execution_count when changing to markdown
                            if ct == "markdown" {
                                cell["outputs"] = Value::Null;
                                cell["execution_count"] = Value::Null;
                            }
                        }
                    } else {
                        // Not changing type, just clear execution_count and reset outputs for code cells
                        cell["execution_count"] = Value::Null;
                        if cell["cell_type"].as_str() == Some("code") {
                            cell["outputs"] = json!([]);
                        }
                    }
                }
            }
            "insert" => {
                let insert_pos = if let Some(idx) = cell_index {
                    idx + 1  // Insert after the found cell
                } else {
                    0  // Insert at beginning if no cell_id specified
                };
                
                // Generate cell ID if nbformat >= 4.5
                let cell_id_val = if nbformat > 4 || (nbformat == 4 && nbformat_minor >= 5) {
                    Some(generate_cell_id())
                } else {
                    cell_id.map(|s| s.to_string())
                };
                
                let mut new_cell = json!({
                    "cell_type": cell_type.unwrap_or("code"),
                    "source": new_source,
                    "metadata": {}
                });
                
                if let Some(id) = cell_id_val {
                    new_cell["id"] = json!(id);
                }
                
                if cell_type.unwrap_or("code") == "code" {
                    new_cell["outputs"] = json!([]);
                }
                
                if insert_pos >= cells.len() {
                    cells.push(new_cell);
                } else {
                    cells.insert(insert_pos, new_cell);
                }
            }
            "delete" => {
                let idx = cell_index.ok_or_else(|| {
                    Error::InvalidInput("Cell ID must be specified for delete operation.".to_string())
                })?;
                cells.remove(idx);
            }
            _ => unreachable!()
        }
        
        // Write the notebook back with formatting matching JavaScript (null, 1)
        let updated_content = serde_json::to_string_pretty(&notebook)
            .map_err(|e| Error::Serialization(e))?;
        
        fs::write(path, updated_content)
            .map_err(|e| Error::Io(e))?;
        
        // Return result matching JavaScript mapToolResultToToolResultBlockParam
        let truncated_source = if new_source.len() > 50 { 
            format!("{}...", &new_source[..50]) 
        } else { 
            new_source.to_string() 
        };
        
        match actual_edit_mode.as_str() {
            "replace" => Ok(format!("Updated cell {} with {}", 
                cell_id.unwrap_or("0"), 
                truncated_source)),
            "insert" => Ok(format!("Inserted cell {} with {}", 
                cell_id.unwrap_or("new"), 
                truncated_source)),
            "delete" => Ok(format!("Deleted cell {}", cell_id.unwrap_or("unknown"))),
            _ => Ok("Unknown edit mode".to_string())
        }
    }
    
    fn action_description(&self, input: &Value) -> String {
        let mode = input["edit_mode"].as_str().unwrap_or("replace");
        if let Some(path) = input["notebook_path"].as_str() {
            format!("Edit notebook ({}): {}", mode, path)
        } else {
            format!("Edit Jupyter notebook ({})", mode)
        }
    }
    
    fn permission_details(&self, input: &Value) -> String {
        let mode = input["edit_mode"].as_str().unwrap_or("replace");
        if let Some(path) = input["notebook_path"].as_str() {
            format!("{} cell in notebook at {}", mode, path)
        } else {
            format!("{} cell in Jupyter notebook", mode)
        }
    }
}

/// Generate a unique cell ID matching JavaScript implementation
fn generate_cell_id() -> String {
    // JavaScript uses: Math.random().toString(36).substring(2, 15)
    // We'll use a similar approach with random alphanumeric characters
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let id: String = (0..13)
        .map(|_| chars[rng.gen_range(0..chars.len())])
        .collect();
    id
}