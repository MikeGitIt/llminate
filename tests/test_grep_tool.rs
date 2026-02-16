use anyhow::Result;
use llminate::ai::tools::{GrepTool, ToolHandler};
use serde_json::json;
use tempfile::TempDir;
use std::fs;

#[tokio::test]
async fn test_grep_basic_search() -> Result<()> {
    // Create test directory with files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();
    
    // Create test files
    fs::write(test_dir.join("file1.rs"), "fn main() {\n    println!(\"Hello\");\n}")?;
    fs::write(test_dir.join("file2.rs"), "fn test() {\n    assert_eq!(1, 1);\n}")?;
    fs::write(test_dir.join("file3.txt"), "This is a test file\nwith multiple lines")?;
    fs::write(test_dir.join("README.md"), "# Test Project\n\nThis project has functions")?;
    
    // Create GrepTool instance
    let grep_tool = GrepTool;
    
    // Test 1: Search for "fn" pattern
    let input = json!({
        "pattern": "fn",
        "path": test_dir.to_string_lossy().to_string()
    });
    
    let result = grep_tool.execute(input, None).await?;
    println!("Test 1 - Search for 'fn': {}", result);
    
    // Should find file1.rs and file2.rs
    assert!(result.contains("file1.rs") || result.contains("file2.rs"));
    assert!(result.contains("Found"));
    assert!(!result.contains("file3.txt")); // Should not contain txt file
    
    Ok(())
}

#[tokio::test]
async fn test_grep_with_include_parameter() -> Result<()> {
    // Create test directory with files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();
    
    // Create test files
    fs::write(test_dir.join("main.rs"), "fn main() {\n    test();\n}")?;
    fs::write(test_dir.join("lib.rs"), "pub fn test() {\n    println!(\"test\");\n}")?;
    fs::write(test_dir.join("test.js"), "function test() {\n    console.log('test');\n}")?;
    fs::write(test_dir.join("test.py"), "def test():\n    print('test')")?;
    
    // Create GrepTool instance
    let grep_tool = GrepTool;
    
    // Test with include parameter matching JavaScript behavior
    let input = json!({
        "pattern": "test",
        "path": test_dir.to_string_lossy().to_string(),
        "include": "*.rs"
    });
    
    let result = grep_tool.execute(input, None).await?;
    println!("Test 2 - Search with include '*.rs': {}", result);
    
    // Should only find .rs files
    assert!(result.contains(".rs"));
    assert!(!result.contains(".js"));
    assert!(!result.contains(".py"));
    
    Ok(())
}

#[tokio::test]
async fn test_grep_with_complex_include_pattern() -> Result<()> {
    // Create test directory with files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();
    
    // Create test files in subdirectories
    fs::create_dir_all(test_dir.join("src"))?;
    fs::create_dir_all(test_dir.join("tests"))?;
    
    fs::write(test_dir.join("src/main.tsx"), "export function App() {}")?;
    fs::write(test_dir.join("src/utils.ts"), "export function helper() {}")?;
    fs::write(test_dir.join("src/styles.css"), ".app { color: red; }")?;
    fs::write(test_dir.join("tests/app.test.tsx"), "describe('App', () => {})")?;
    
    // Create GrepTool instance
    let grep_tool = GrepTool;
    
    // Test with complex include pattern like JavaScript: "*.{ts,tsx}"
    let input = json!({
        "pattern": "function",
        "path": test_dir.to_string_lossy().to_string(),
        "include": "*.{ts,tsx}"
    });
    
    let result = grep_tool.execute(input, None).await?;
    println!("Test 3 - Search with include '*.{{ts,tsx}}': {}", result);
    
    // Should find .ts and .tsx files
    assert!(result.contains(".tsx") || result.contains(".ts"));
    assert!(!result.contains(".css"));
    
    Ok(())
}

#[tokio::test]
async fn test_grep_no_matches() -> Result<()> {
    // Create test directory with files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();
    
    // Create test files
    fs::write(test_dir.join("file1.txt"), "Some content here")?;
    fs::write(test_dir.join("file2.txt"), "Another file content")?;
    
    // Create GrepTool instance
    let grep_tool = GrepTool;
    
    // Search for non-existent pattern
    let input = json!({
        "pattern": "nonexistentpattern",
        "path": test_dir.to_string_lossy().to_string()
    });
    
    let result = grep_tool.execute(input, None).await?;
    println!("Test 4 - No matches: {}", result);
    
    // Should return "No files found"
    assert_eq!(result, "No files found");
    
    Ok(())
}

#[tokio::test]
async fn test_grep_multiple_include_patterns() -> Result<()> {
    // Create test directory with files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();
    
    // Create test files
    fs::write(test_dir.join("main.js"), "console.log('test');")?;
    fs::write(test_dir.join("utils.ts"), "export const test = 'test';")?;
    fs::write(test_dir.join("app.jsx"), "return <div>test</div>;")?;
    fs::write(test_dir.join("style.css"), ".test { color: blue; }")?;
    fs::write(test_dir.join("config.json"), "{\"test\": true}")?;
    
    // Create GrepTool instance
    let grep_tool = GrepTool;
    
    // Test with multiple patterns separated by space (JavaScript behavior)
    let input = json!({
        "pattern": "test",
        "path": test_dir.to_string_lossy().to_string(),
        "include": "*.js *.ts *.jsx"
    });
    
    let result = grep_tool.execute(input, None).await?;
    println!("Test 5 - Multiple include patterns: {}", result);
    
    // Should find .js, .ts, and .jsx files
    assert!(result.contains(".js") || result.contains(".ts") || result.contains(".jsx"));
    assert!(!result.contains(".css"));
    assert!(!result.contains(".json"));
    
    Ok(())
}

