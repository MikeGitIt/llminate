/// Git-related prompts extracted from the JavaScript implementation
/// Contains prompts for git history analysis and CLAUDE.md creation

/// Returns the git history analysis prompt
/// This prompt is used to analyze git history and identify frequently modified core files
pub fn get_git_history_prompt() -> &'static str {
    "You are an expert at analyzing git history. Given a list of files and their modification counts, return exactly five filenames that are frequently modified and represent core application logic (not auto-generated files, dependencies, or configuration). Make sure filenames are diverse, not all in the same folder, and are a mix of user and other users. Return only the filenames' basenames (without the path) separated by newlines with no explanation."
}

/// Returns the CLAUDE.md creation prompt
/// This prompt is used to generate or update CLAUDE.md files for repositories
pub fn get_claude_md_prompt() -> &'static str {
    r#"Please analyze this codebase and create a CLAUDE.md file, which will be given to future instances of Claude Code to operate in this repository.
            
What to add:
1. Commands that will be commonly used, such as how to build, lint, and run tests. Include the necessary commands to develop in this codebase, such as how to run a single test.
2. High-level code architecture and structure so that future instances can be productive more quickly. Focus on the "big picture" architecture that requires reading multiple files to understand

Usage notes:
- If there's already a CLAUDE.md, suggest improvements to it.
- When you make the initial CLAUDE.md, do not repeat yourself and do not include obvious instructions like "Provide helpful error messages to users", "Write unit tests for all new utilities", "Never include sensitive information (API keys, tokens) in code or commits" 
- Avoid listing every component or file structure that can be easily discovered
- Don't include generic development practices
- If there are Cursor rules (in .cursor/rules/ or .cursorrules) or Copilot rules (in .github/copilot-instructions.md), make sure to include the important parts.
- If there is a README.md, make sure to include the important parts. 
- Do not make up information such as "Common Development Tasks", "Tips for Development", "Support and Documentation" unless this is expressly included in other files that you read.
- Be sure to prefix the file with the following text:

```
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.
```"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_history_prompt_not_empty() {
        let prompt = get_git_history_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("git history"));
        assert!(prompt.contains("five filenames"));
        assert!(prompt.contains("basenames"));
    }

    #[test]
    fn test_claude_md_prompt_not_empty() {
        let prompt = get_claude_md_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("CLAUDE.md"));
        assert!(prompt.contains("Commands that will be commonly used"));
        assert!(prompt.contains("High-level code architecture"));
    }

    #[test]
    fn test_claude_md_prompt_contains_prefix_requirement() {
        let prompt = get_claude_md_prompt();
        assert!(prompt.contains("# CLAUDE.md"));
        assert!(prompt.contains("This file provides guidance to Claude Code"));
    }
}