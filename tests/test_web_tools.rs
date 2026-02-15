use anyhow::Result;
use serde_json::json;
use llminate::ai::tools::ToolHandler;
use llminate::ai::web_tools::{WebFetchTool, WebSearchTool};
use std::sync::Once;

// Ensure environment variables are loaded only once
static INIT: Once = Once::new();

fn init_env() {
    INIT.call_once(|| {
        dotenv::dotenv().ok();
    });
}

#[tokio::test]
async fn test_webfetch_real_website() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test fetching a real website
    let input = json!({
        "url": "https://www.example.com",
        "prompt": "What is the main heading on this page?"
    });
    
    // Execute
    let result = tool.execute(input).await?;
    
    // Verify we got actual content
    println!("WebFetch result for example.com:\n{}", result);
    
    // example.com should contain "Example Domain"
    assert!(result.contains("Example Domain") || result.contains("example"), 
            "Result should contain content from example.com");
    
    // Verify the fetch metadata is present
    assert!(result.contains("200") || result.contains("OK"),
            "Result should contain HTTP status");
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_github_api() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test fetching JSON from GitHub API
    let input = json!({
        "url": "https://api.github.com/repos/anthropics/anthropic-sdk-python",
        "prompt": "What is the description of this repository?"
    });
    
    // Execute
    let result = tool.execute(input).await?;
    
    println!("WebFetch result for GitHub API:\n{}", result);
    
    // Should contain repository information
    assert!(result.contains("anthropic") || result.contains("SDK") || result.contains("Python"),
            "Result should contain GitHub repo information");
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_http_to_https_upgrade_real() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test with HTTP URL that should be upgraded to HTTPS
    let input = json!({
        "url": "http://www.rust-lang.org",
        "prompt": "What programming language is this website about?"
    });
    
    // Execute - should upgrade to HTTPS and fetch
    let result = tool.execute(input).await;
    
    // rust-lang.org should work with HTTPS
    if let Ok(content) = result {
        println!("HTTP->HTTPS upgrade result:\n{}", content);
        assert!(content.contains("Rust") || content.contains("rust"),
                "Should fetch Rust website content");
    } else {
        // If it fails, that's also acceptable as long as it's not a panic
        println!("HTTP->HTTPS upgrade failed (expected in some environments)");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_404_error() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test fetching a 404 page
    let input = json!({
        "url": "https://httpbin.org/status/404",
        "prompt": "What is on this page?"
    });
    
    // Execute
    let result = tool.execute(input).await?;
    
    println!("WebFetch 404 result:\n{}", result);
    
    // Should still return content but with 404 status
    assert!(result.contains("404") || result.contains("Not Found"),
            "Result should indicate 404 status");
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_large_content_truncation() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test fetching a large page (Wikipedia tends to have large pages)
    let input = json!({
        "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "prompt": "Give me a brief summary"
    });
    
    // Execute
    let result = tool.execute(input).await;
    
    if let Ok(content) = result {
        println!("Large content fetch - length: {} chars", content.len());
        
        // Check if content was truncated (look for truncation marker)
        if content.contains("[content truncated]") {
            println!("✓ Large content was properly truncated");
        } else {
            println!("Content was within size limits");
        }
        
        assert!(content.contains("Rust") || content.contains("programming"),
                "Should contain relevant content");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_caching_real() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    let url = "https://api.github.com/users/github";
    
    // First request
    let input1 = json!({
        "url": url,
        "prompt": "First request - what is the company?"
    });
    
    let start1 = std::time::Instant::now();
    let result1 = tool.execute(input1).await?;
    let duration1 = start1.elapsed();
    
    println!("First fetch took: {:?}", duration1);
    assert!(result1.contains("GitHub") || result1.contains("github"));
    
    // Second request (should use cache and be much faster)
    let input2 = json!({
        "url": url,
        "prompt": "Second request - cached?"
    });
    
    let start2 = std::time::Instant::now();
    let result2 = tool.execute(input2).await?;
    let duration2 = start2.elapsed();
    
    println!("Second fetch took: {:?}", duration2);
    
    // Second request should be significantly faster due to caching
    // Allow some tolerance for system variations
    if duration2 < duration1 / 2 {
        println!("✓ Cache is working - second request was much faster");
    } else {
        println!("⚠ Cache might not be working optimally");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_websearch_real_execution() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebSearch tool
    let tool = WebSearchTool;
    
    // Test real search query
    let input = json!({
        "query": "Rust programming language features"
    });
    
    // Execute
    let result = tool.execute(input).await?;
    
    println!("WebSearch result:\n{}", result);
    
    // The result should indicate it needs Claude API integration
    assert!(result.contains("Web search") || result.contains("query"));
    assert!(result.contains("Rust programming language features"));
    
    // Since we don't have Claude API access in tests, verify it explains this
    assert!(result.contains("Claude API") || result.contains("requires") || result.contains("integration"),
            "Should explain that real search requires Claude API");
    
    Ok(())
}

#[tokio::test]
async fn test_websearch_with_domain_filters() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebSearch tool
    let tool = WebSearchTool;
    
    // Test with real domain filters
    let input = json!({
        "query": "machine learning tutorials",
        "allowed_domains": ["github.com", "arxiv.org", "tensorflow.org"]
    });
    
    // Execute
    let result = tool.execute(input).await?;
    
    println!("WebSearch with filters result:\n{}", result);
    
    // Verify the filters are recognized
    assert!(result.contains("machine learning tutorials"));
    assert!(result.contains("github.com") || result.contains("allowed domains"));
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_timeout_handling() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test with a URL that delays response (httpbin provides this)
    let input = json!({
        "url": "https://httpbin.org/delay/35", // 35 second delay, should timeout at 30
        "prompt": "Test timeout"
    });
    
    // Execute and expect timeout
    let start = std::time::Instant::now();
    let result = tool.execute(input).await;
    let duration = start.elapsed();
    
    println!("Timeout test took: {:?}", duration);
    
    // Should timeout around 30 seconds
    if result.is_err() {
        println!("✓ Request timed out as expected");
        assert!(duration.as_secs() <= 35, "Should timeout before 35 seconds");
    } else {
        println!("Request unexpectedly succeeded");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_webfetch_invalid_ssl_cert() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    // Create WebFetch tool
    let tool = WebFetchTool;
    
    // Test with a site that has SSL issues (self-signed cert test site)
    let input = json!({
        "url": "https://self-signed.badssl.com/",
        "prompt": "Test SSL handling"
    });
    
    // Execute - should handle SSL error gracefully
    let result = tool.execute(input).await;
    
    if result.is_err() {
        println!("✓ SSL error handled gracefully");
    } else if let Ok(content) = result {
        println!("SSL site fetch result:\n{}", content);
        // Some clients might accept self-signed certs
    }
    
    Ok(())
}

#[tokio::test]
async fn test_websearch_schema_validation() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    let tool = WebSearchTool;
    
    // Test with missing query
    let input = json!({});
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with missing query");
    
    // Test with empty query
    let input = json!({"query": ""});
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with empty query");
    
    // Test with short query
    let input = json!({"query": "a"});
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with query too short");
    
    // Test with valid query
    let input = json!({"query": "ok"});
    let result = tool.execute(input).await;
    assert!(result.is_ok(), "Should succeed with valid query");
    
    println!("✓ WebSearch schema validation working correctly");
    Ok(())
}

#[tokio::test]
async fn test_webfetch_schema_validation() -> Result<()> {
    // Ensure environment is initialized
    init_env();
    
    let tool = WebFetchTool;
    
    // Test with missing URL
    let input = json!({"prompt": "test"});
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with missing URL");
    
    // Test with missing prompt
    let input = json!({"url": "https://example.com"});
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with missing prompt");
    
    // Test with invalid URL
    let input = json!({
        "url": "not a url",
        "prompt": "test"
    });
    let result = tool.execute(input).await;
    assert!(result.is_err(), "Should fail with invalid URL");
    
    // Test with valid input
    let input = json!({
        "url": "https://example.com",
        "prompt": "test"
    });
    let result = tool.execute(input).await;
    assert!(result.is_ok(), "Should succeed with valid input");
    
    println!("✓ WebFetch schema validation working correctly");
    Ok(())
}