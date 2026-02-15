use anyhow::Result;
use serde_json::json;
use tempfile::TempDir;
use std::fs;
use std::path::Path;
use llminate::{
    ai::tools::ToolHandler,
    ai::notebook_tools::{NotebookReadTool, NotebookEditTool},
};

/// Create a test notebook with some cells
fn create_test_notebook(path: &Path) -> Result<()> {
    let notebook = json!({
        "cells": [
            {
                "cell_type": "markdown",
                "source": ["# Test Notebook\n", "This is a test notebook for unit tests.\n"],
                "metadata": {},
                "id": "markdown-1"
            },
            {
                "cell_type": "code",
                "source": "import numpy as np\nprint('Hello, World!')",
                "outputs": [
                    {
                        "output_type": "stream",
                        "name": "stdout",
                        "text": "Hello, World!\n"
                    }
                ],
                "execution_count": 1,
                "metadata": {},
                "id": "code-1"
            },
            {
                "cell_type": "code",
                "source": ["def factorial(n):\n", "    if n <= 1:\n", "        return 1\n", "    return n * factorial(n-1)\n", "\n", "print(factorial(5))"],
                "outputs": [
                    {
                        "output_type": "stream",
                        "name": "stdout",
                        "text": "120\n"
                    }
                ],
                "execution_count": 2,
                "metadata": {},
                "id": "code-2"
            }
        ],
        "metadata": {
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3"
            },
            "language_info": {
                "name": "python",
                "version": "3.9.0"
            }
        },
        "nbformat": 4,
        "nbformat_minor": 5
    });
    
    let content = serde_json::to_string_pretty(&notebook)?;
    fs::write(path, content)?;
    Ok(())
}

#[tokio::test]
async fn test_notebook_read_all_cells() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookReadTool;
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?
    });
    
    let result = tool.execute(input).await?;
    
    // Verify the output contains expected content in XML-like format (matches JavaScript)
    assert!(result.contains("<cell id="));
    assert!(result.contains("</cell"));
    assert!(result.contains("# Test Notebook"));
    assert!(result.contains("This is a test notebook"));
    assert!(result.contains("import numpy as np"));
    assert!(result.contains("Hello, World!"));
    assert!(result.contains("def factorial"));
    
    // Verify cell types are shown in XML format
    assert!(result.contains("<cell_type>markdown</cell_type>"));
    
    // Verify outputs are shown
    assert!(result.contains("120"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_read_specific_cell_by_index() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookReadTool;
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "1"
    });
    
    // JavaScript would fail here - it doesn't support numeric indices in execution
    let result = tool.execute(input).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cell with ID \"1\" not found in notebook"));
    
    // Test with actual cell ID instead to verify the assertions below work
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "code-1"
    });
    let result = tool.execute(input).await?;
    
    // Should only contain the second cell (code-1) in XML format
    assert!(result.contains("<cell id=\"code-1\">"));
    assert!(result.contains("import numpy as np"));
    assert!(result.contains("Hello, World!"));
    assert!(!result.contains("# Test Notebook"));
    assert!(!result.contains("def factorial"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_read_specific_cell_by_id() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookReadTool;
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "code-2"
    });
    
    let result = tool.execute(input).await?;
    
    // Should only contain the factorial cell in XML format
    assert!(result.contains("<cell id=\"code-2\">"));
    assert!(result.contains("def factorial"));
    assert!(result.contains("120"));
    assert!(!result.contains("Hello, World!"));
    assert!(!result.contains("# Test Notebook"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_read_nonexistent_file() -> Result<()> {
    let tool = NotebookReadTool;
    let input = json!({
        "notebook_path": "/nonexistent/path/notebook.ipynb"
    });
    
    let result = tool.execute(input).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid notebook path"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_read_invalid_extension() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Not a notebook")?;
    
    let tool = NotebookReadTool;
    let input = json!({
        "notebook_path": file_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?
    });
    
    let result = tool.execute(input).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must be a Jupyter notebook"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_replace_cell() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookEditTool;
    
    // Replace the first code cell
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "1",
        "new_source": "# Modified code\nprint('Modified!')"
    });
    
    let result = tool.execute(input).await?;
    assert!(result.contains("Updated cell"));
    
    // Read back and verify
    let content = fs::read_to_string(&notebook_path)?;
    let notebook: serde_json::Value = serde_json::from_str(&content)?;
    
    let cell = &notebook["cells"][1];
    let source = if cell["source"].is_string() {
        cell["source"].as_str().ok_or_else(|| anyhow::anyhow!("Expected string"))?.to_string()
    } else {
        cell["source"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    
    assert!(source.contains("Modified code"));
    assert!(source.contains("Modified!"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_insert_cell() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookEditTool;
    
    // Insert a new cell after the first one
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "0",
        "new_source": "# Inserted markdown cell\nThis was inserted!",
        "cell_type": "markdown",
        "edit_mode": "insert"
    });
    
    let result = tool.execute(input).await?;
    assert!(result.contains("Inserted cell"));
    
    // Read back and verify
    let content = fs::read_to_string(&notebook_path)?;
    let notebook: serde_json::Value = serde_json::from_str(&content)?;
    
    // Should now have 4 cells
    assert_eq!(notebook["cells"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?.len(), 4);
    
    // The second cell should be the inserted one
    let cell = &notebook["cells"][1];
    assert_eq!(cell["cell_type"], "markdown");
    let source = if cell["source"].is_string() {
        cell["source"].as_str().ok_or_else(|| anyhow::anyhow!("Expected string"))?.to_string()
    } else {
        cell["source"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    assert!(source.contains("Inserted markdown"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_delete_cell() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookEditTool;
    
    // Delete the second cell
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "1",
        "new_source": "",  // Required but not used for delete
        "edit_mode": "delete"
    });
    
    let result = tool.execute(input).await?;
    assert!(result.contains("Deleted cell"));
    
    // Read back and verify
    let content = fs::read_to_string(&notebook_path)?;
    let notebook: serde_json::Value = serde_json::from_str(&content)?;
    
    // Should now have 2 cells
    assert_eq!(notebook["cells"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?.len(), 2);
    
    // The second cell should now be the factorial one
    let cell = &notebook["cells"][1];
    let source = if cell["source"].is_string() {
        cell["source"].as_str().ok_or_else(|| anyhow::anyhow!("Expected string"))?.to_string()
    } else {
        cell["source"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    assert!(source.contains("factorial"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_change_cell_type() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookEditTool;
    
    // Change a code cell to markdown
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "code-1",
        "new_source": "# This was code\nNow it's markdown!",
        "cell_type": "markdown"
    });
    
    let result = tool.execute(input).await?;
    assert!(result.contains("Updated cell"));
    
    // Read back and verify
    let content = fs::read_to_string(&notebook_path)?;
    let notebook: serde_json::Value = serde_json::from_str(&content)?;
    
    // Find the cell with id "code-1"
    let cell = notebook["cells"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?
        .iter()
        .find(|c| c["id"] == "code-1")
        .ok_or_else(|| anyhow::anyhow!("Cell not found"))?;
    
    assert_eq!(cell["cell_type"], "markdown");
    assert!(cell["outputs"].is_null());  // Outputs should be cleared
    assert!(cell["execution_count"].is_null());  // Execution count should be cleared
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_invalid_cell_id() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("test.ipynb");
    create_test_notebook(&notebook_path)?;
    
    let tool = NotebookEditTool;
    
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "cell_id": "nonexistent-id",
        "new_source": "This won't work"
    });
    
    let result = tool.execute(input).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found in notebook"));
    
    Ok(())
}

#[tokio::test]
async fn test_notebook_edit_empty_notebook() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let notebook_path = temp_dir.path().join("empty.ipynb");
    
    // Create an empty notebook
    let notebook = json!({
        "cells": [],
        "metadata": {},
        "nbformat": 4,
        "nbformat_minor": 5
    });
    fs::write(&notebook_path, serde_json::to_string(&notebook)?)?;
    
    let tool = NotebookEditTool;
    
    // Insert a cell at the beginning
    let input = json!({
        "notebook_path": notebook_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        "new_source": "# First cell",
        "cell_type": "markdown",
        "edit_mode": "insert"
    });
    
    let result = tool.execute(input).await?;
    assert!(result.contains("Inserted cell"));
    
    // Verify the notebook now has one cell
    let content = fs::read_to_string(&notebook_path)?;
    let updated: serde_json::Value = serde_json::from_str(&content)?;
    assert_eq!(updated["cells"].as_array().ok_or_else(|| anyhow::anyhow!("Expected array"))?.len(), 1);
    
    Ok(())
}