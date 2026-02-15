/// GitHub-related prompts for PR comments and code review
/// 
/// This module contains the exact prompts extracted from the JavaScript implementation
/// for GitHub PR operations.

/// Returns the prompt for fetching GitHub PR comments
/// 
/// This prompt provides instructions for:
/// - Fetching PR comments using gh CLI commands
/// - Using GitHub API endpoints for PR and review comments
/// - Formatting comments in a readable way
/// - Handling comment threading and context
pub fn get_pr_comments_prompt(additional_input: Option<&str>) -> String {
    let additional_text = match additional_input {
        Some(input) => format!("\n\nAdditional user input: {}", input),
        None => String::new(),
    };

    format!(
        r#"You are an AI assistant integrated into a git-based version control system. Your task is to fetch and display comments from a GitHub pull request.

Follow these steps:

1. Use `gh pr view --json number,headRepository` to get the PR number and repository info
2. Use `gh api /repos/{{owner}}/{{repo}}/issues/{{number}}/comments` to get PR-level comments
3. Use `gh api /repos/{{owner}}/{{repo}}/pulls/{{number}}/comments` to get review comments. Pay particular attention to the following fields: `body`, `diff_hunk`, `path`, `line`, etc. If the comment references some code, consider fetching it using eg `gh api /repos/{{owner}}/{{repo}}/contents/{{path}}?ref={{branch}} | jq .content -r | base64 -d`
4. Parse and format all comments in a readable way
5. Return ONLY the formatted comments, with no additional text

Format the comments as:

## Comments

[For each comment thread:]
- @author file.ts#line:
  ```diff
  [diff_hunk from the API response]
  ```
  > quoted comment text
  
  [any replies indented]

If there are no comments, return "No comments found."

Remember:
1. Only show the actual comments, no explanatory text
2. Include both PR-level and code review comments
3. Preserve the threading/nesting of comment replies
4. Show the file and line number context for code review comments
5. Use jq to parse the JSON responses from the GitHub API{}"#,
        additional_text
    )
}

/// Returns the prompt for expert code review
/// 
/// This prompt provides instructions for:
/// - Analyzing GitHub PRs with thorough code review
/// - Using gh CLI to get PR details and diffs
/// - Providing comprehensive feedback on code quality
/// - Focusing on correctness, conventions, performance, and security
pub fn get_code_review_prompt(pr_number: Option<&str>) -> String {
    let pr_text = match pr_number {
        Some(number) => format!("\n\nPR number: {}", number),
        None => String::new(),
    };

    format!(
        r#"You are an expert code reviewer. Follow these steps:

1. If no PR number is provided in the args, use Bash("gh pr list") to show open PRs
2. If a PR number is provided, use Bash("gh pr view <number>") to get PR details
3. Use Bash("gh pr diff <number>") to get the diff
4. Analyze the changes and provide a thorough code review that includes:
   - Overview of what the PR does
   - Analysis of code quality and style
   - Specific suggestions for improvements
   - Any potential issues or risks

Keep your review concise but thorough. Focus on:
- Code correctness
- Following project conventions
- Performance implications
- Test coverage
- Security considerations

Format your review with clear sections and bullet points.{}"#,
        pr_text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_comments_prompt_without_input() {
        let prompt = get_pr_comments_prompt(None);
        
        assert!(prompt.contains("You are an AI assistant integrated into a git-based version control system"));
        assert!(prompt.contains("Follow these steps:"));
        assert!(prompt.contains("gh pr view --json number,headRepository"));
        assert!(prompt.contains("## Comments"));
        assert!(!prompt.contains("Additional user input:"));
    }

    #[test]
    fn test_pr_comments_prompt_with_input() {
        let prompt = get_pr_comments_prompt(Some("Show only recent comments"));
        
        assert!(prompt.contains("You are an AI assistant integrated into a git-based version control system"));
        assert!(prompt.contains("Additional user input: Show only recent comments"));
    }

    #[test]
    fn test_code_review_prompt_without_pr() {
        let prompt = get_code_review_prompt(None);
        
        assert!(prompt.contains("You are an expert code reviewer"));
        assert!(prompt.contains("Follow these steps:"));
        assert!(prompt.contains("gh pr list"));
        assert!(prompt.contains("Code correctness"));
        assert!(!prompt.contains("PR number:"));
    }

    #[test]
    fn test_code_review_prompt_with_pr() {
        let prompt = get_code_review_prompt(Some("123"));
        
        assert!(prompt.contains("You are an expert code reviewer"));
        assert!(prompt.contains("PR number: 123"));
    }
}