// Skill tool - Execute skills/slash commands
// Matches JavaScript implementation at cli-jsdef-fixed.js:443790-443981
//
// Parameters:
//   - skill: String (the skill name, e.g., "commit", "review-pr", "pdf")
//   - args: Option<String> (optional arguments for the skill) - NOTE: JS only uses skill name
//
// Returns: Skill execution result with success status, command name, allowed tools, and model override
//
// Skills are loaded from:
// 1. Built-in skills (commit, review-pr, etc.)
// 2. ~/.claude/skills/ directory (user settings)
// 3. .claude/skills/ project directory (project settings)

use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs as async_fs;
use tokio_util::sync::CancellationToken;

/// Skill type - matches JavaScript skill types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    Prompt,
    Command,
}

/// Skill source - where the skill was loaded from
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillSource {
    #[serde(rename = "policySettings")]
    PolicySettings,
    #[serde(rename = "userSettings")]
    UserSettings,
    #[serde(rename = "projectSettings")]
    ProjectSettings,
    BuiltIn,
}

impl SkillSource {
    pub fn as_str(&self) -> &str {
        match self {
            SkillSource::PolicySettings => "policySettings",
            SkillSource::UserSettings => "userSettings",
            SkillSource::ProjectSettings => "projectSettings",
            SkillSource::BuiltIn => "built-in",
        }
    }
}

/// Skill frontmatter metadata from SKILL.md files
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkillFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,
    pub when_to_use: Option<String>,
    pub version: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,
}

/// Skill definition - matches JavaScript skill structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    #[serde(rename = "type")]
    pub skill_type: SkillType,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub is_skill: bool,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default)]
    pub is_hidden: bool,
    pub source: SkillSource,
    /// Path to the skill directory (for prompt-based skills)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_dir: Option<PathBuf>,
    /// The content of the SKILL.md file (for prompt-based skills)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl Skill {
    /// Get user-facing name (from frontmatter or directory name)
    pub fn user_facing_name(&self) -> &str {
        &self.name
    }

    /// Get prompt for command execution
    pub fn get_prompt_for_command(&self, arguments: Option<&str>) -> Result<String> {
        let Some(content) = &self.content else {
            return Err(Error::InvalidInput("Skill has no content".to_string()));
        };

        // For built-in skills, skill_dir is None - we just use the content directly
        let mut prompt = if let Some(skill_dir) = &self.skill_dir {
            format!(
                "Base directory for this skill: {}\n\n{}",
                skill_dir.display(),
                content
            )
        } else {
            content.clone()
        };

        // Handle $ARGUMENTS placeholder or append arguments
        if let Some(args) = arguments {
            if prompt.contains("$ARGUMENTS") {
                prompt = prompt.replace("$ARGUMENTS", args);
            } else {
                prompt.push_str(&format!("\n\nARGUMENTS: {}", args));
            }
        }

        Ok(prompt)
    }
}

/// Skill execution result - matches JavaScript output schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    pub success: bool,
    #[serde(rename = "commandName")]
    pub command_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "allowedTools")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Built-in skills that are always available
fn get_builtin_skills() -> HashMap<String, Skill> {
    let mut skills = HashMap::new();

    // commit skill - creates git commits
    skills.insert(
        "commit".to_string(),
        Skill {
            skill_type: SkillType::Prompt,
            name: "commit".to_string(),
            description: "Create a git commit with a well-crafted message".to_string(),
            allowed_tools: Some(vec!["Bash".to_string(), "Read".to_string()]),
            argument_hint: Some("[message]".to_string()),
            when_to_use: Some("When the user wants to commit changes".to_string()),
            version: Some("1.0.0".to_string()),
            model: None,
            is_skill: true,
            disable_model_invocation: false,
            is_hidden: false,
            source: SkillSource::BuiltIn,
            skill_dir: None,
            content: Some(r#"Create a git commit for the staged changes.

If the user provided a commit message, use that. Otherwise, analyze the changes and create an appropriate commit message following these guidelines:

1. Use conventional commit format: type(scope): description
2. Types: feat, fix, docs, style, refactor, test, chore
3. Keep the first line under 72 characters
4. Add a blank line after the subject, then a detailed body if needed

Steps:
1. Run `git status` to see what's staged
2. Run `git diff --cached` to see the actual changes
3. Craft a commit message based on the changes
4. Create the commit with `git commit -m "message"`
"#.to_string()),
        },
    );

    // review-pr skill - reviews pull requests
    skills.insert(
        "review-pr".to_string(),
        Skill {
            skill_type: SkillType::Prompt,
            name: "review-pr".to_string(),
            description: "Review a GitHub pull request".to_string(),
            allowed_tools: Some(vec![
                "Bash".to_string(),
                "Read".to_string(),
                "WebFetch".to_string(),
            ]),
            argument_hint: Some("<pr-number-or-url>".to_string()),
            when_to_use: Some("When the user wants to review a PR".to_string()),
            version: Some("1.0.0".to_string()),
            model: None,
            is_skill: true,
            disable_model_invocation: false,
            is_hidden: false,
            source: SkillSource::BuiltIn,
            skill_dir: None,
            content: Some(r#"Review the specified GitHub pull request.

$ARGUMENTS

Steps:
1. Use `gh pr view <number> --json title,body,files,additions,deletions` to get PR details
2. Use `gh pr diff <number>` to see the actual changes
3. Analyze the changes for:
   - Code quality and style
   - Potential bugs or issues
   - Test coverage
   - Documentation
4. Provide a constructive review with specific suggestions
"#.to_string()),
        },
    );

    // pdf skill - extracts content from PDF files
    skills.insert(
        "pdf".to_string(),
        Skill {
            skill_type: SkillType::Prompt,
            name: "pdf".to_string(),
            description: "Extract and analyze content from a PDF file".to_string(),
            allowed_tools: Some(vec!["Read".to_string(), "Bash".to_string()]),
            argument_hint: Some("<file-path>".to_string()),
            when_to_use: Some("When the user wants to read or analyze a PDF".to_string()),
            version: Some("1.0.0".to_string()),
            model: None,
            is_skill: true,
            disable_model_invocation: false,
            is_hidden: false,
            source: SkillSource::BuiltIn,
            skill_dir: None,
            content: Some(r#"Extract and analyze content from the specified PDF file.

$ARGUMENTS

Use the Read tool to read the PDF file. The tool will automatically extract text content from PDFs.
"#.to_string()),
        },
    );

    skills
}

/// Skill manager - handles skill loading and lookup
#[derive(Debug)]
pub struct SkillManager {
    skills: HashMap<String, Skill>,
    loaded: bool,
}

impl SkillManager {
    pub fn new() -> Self {
        // Start with built-in skills available immediately
        Self {
            skills: get_builtin_skills(),
            loaded: false,
        }
    }

    /// Load all skills from various sources (user and project directories)
    pub async fn load_skills(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        // Built-in skills are already loaded in new(), add custom skills

        // Load from ~/.claude/skills/ (user settings)
        if let Some(home) = dirs::home_dir() {
            let user_skills_dir = home.join(".claude").join("skills");
            if user_skills_dir.exists() {
                let skills = load_skills_from_directory(&user_skills_dir, SkillSource::UserSettings).await;
                for skill in skills {
                    self.skills.insert(skill.name.clone(), skill);
                }
            }
        }

        // Load from .claude/skills/ (project settings)
        if let Ok(cwd) = std::env::current_dir() {
            let project_skills_dir = cwd.join(".claude").join("skills");
            if project_skills_dir.exists() {
                let skills = load_skills_from_directory(&project_skills_dir, SkillSource::ProjectSettings).await;
                for skill in skills {
                    self.skills.insert(skill.name.clone(), skill);
                }
            }
        }

        self.loaded = true;
        Ok(())
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        // Normalize name (remove leading /)
        let normalized = name.strip_prefix('/').unwrap_or(name);
        self.skills.get(normalized)
    }

    /// Check if a skill exists
    pub fn skill_exists(&self, name: &str) -> bool {
        let normalized = name.strip_prefix('/').unwrap_or(name);
        self.skills.contains_key(normalized)
    }

    /// Get all available skills
    pub fn get_all_skills(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Format skills for the system prompt
    pub fn format_available_skills(&self) -> String {
        let mut output = String::new();

        for skill in self.skills.values() {
            if skill.is_hidden {
                continue;
            }

            let arg_hint = skill
                .argument_hint
                .as_ref()
                .map(|h| format!(" {}", h))
                .unwrap_or_default();

            let when_to_use = skill
                .when_to_use
                .as_ref()
                .map(|w| format!("- {}", w))
                .unwrap_or_default();

            output.push_str(&format!(
                "- /{}{}:{} {}\n",
                skill.name, arg_hint, skill.description, when_to_use
            ));
        }

        output
    }
}

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Load skills from a directory
async fn load_skills_from_directory(dir: &Path, source: SkillSource) -> Vec<Skill> {
    let mut skills = Vec::new();

    // Check if the directory itself has a SKILL.md
    let skill_md = dir.join("SKILL.md");
    if skill_md.exists() {
        if let Some(skill) = load_skill_from_file(&skill_md, dir, &source).await {
            skills.push(skill);
        }
        return skills;
    }

    // Otherwise, look for subdirectories with SKILL.md files
    let Ok(mut entries) = async_fs::read_dir(dir).await else {
        return skills;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            if let Some(skill) = load_skill_from_file(&skill_md, &path, &source).await {
                skills.push(skill);
            }
        }
    }

    skills
}

/// Load a skill from a SKILL.md file
async fn load_skill_from_file(file_path: &Path, skill_dir: &Path, source: &SkillSource) -> Option<Skill> {
    let content = async_fs::read_to_string(file_path).await.ok()?;

    // Parse frontmatter and content
    let (frontmatter, body) = parse_skill_frontmatter(&content);

    // Get skill name from frontmatter or directory name
    let name = frontmatter
        .name
        .clone()
        .or_else(|| {
            skill_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
        })?;

    // Get description from frontmatter or extract from body
    let description = frontmatter
        .description
        .clone()
        .unwrap_or_else(|| extract_description_from_content(&body));

    // Check if model invocation is disabled
    let disable_model_invocation = frontmatter.disable_model_invocation.unwrap_or(false);

    // Get model override (ignore "inherit")
    let model = frontmatter.model.clone().and_then(|m| {
        if m == "inherit" {
            None
        } else {
            Some(m)
        }
    });

    Some(Skill {
        skill_type: SkillType::Prompt,
        name,
        description: format!("{} ({})", description, source.as_str()),
        allowed_tools: frontmatter.allowed_tools.clone(),
        argument_hint: frontmatter.argument_hint.clone(),
        when_to_use: frontmatter.when_to_use.clone(),
        version: frontmatter.version.clone(),
        model,
        is_skill: true,
        disable_model_invocation,
        is_hidden: true, // Custom skills are hidden from the default list
        source: source.clone(),
        skill_dir: Some(skill_dir.to_path_buf()),
        content: Some(body),
    })
}

/// Parse YAML frontmatter from skill content
fn parse_skill_frontmatter(content: &str) -> (SkillFrontmatter, String) {
    // Check for YAML frontmatter (---\n...\n---\n)
    if !content.starts_with("---\n") {
        return (SkillFrontmatter::default(), content.to_string());
    }

    // Find the end of frontmatter
    if let Some(end_idx) = content[4..].find("\n---") {
        let yaml_content = &content[4..4 + end_idx];
        let body = &content[4 + end_idx + 4..];

        // Parse YAML frontmatter
        match serde_yaml::from_str::<SkillFrontmatter>(yaml_content) {
            Ok(fm) => return (fm, body.trim_start().to_string()),
            Err(e) => {
                tracing::warn!("Failed to parse skill frontmatter: {}", e);
            }
        }
    }

    (SkillFrontmatter::default(), content.to_string())
}

/// Extract a description from the first meaningful line of content
fn extract_description_from_content(content: &str) -> String {
    const MAX_DESCRIPTION_LEN: usize = 100;

    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty lines and headers
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Use first non-empty, non-header line
        if trimmed.len() <= MAX_DESCRIPTION_LEN {
            return trimmed.to_string();
        }
        return format!("{}...", &trimmed[..MAX_DESCRIPTION_LEN - 3]);
    }

    "Skill".to_string()
}

/// Global skill manager instance
static SKILL_MANAGER: tokio::sync::OnceCell<tokio::sync::RwLock<SkillManager>> =
    tokio::sync::OnceCell::const_new();

/// Get or initialize the global skill manager
pub async fn get_skill_manager() -> &'static tokio::sync::RwLock<SkillManager> {
    SKILL_MANAGER
        .get_or_init(|| async {
            let mut manager = SkillManager::new();
            if let Err(e) = manager.load_skills().await {
                tracing::error!("Failed to load skills: {}", e);
            }
            tokio::sync::RwLock::new(manager)
        })
        .await
}

/// Skill tool - Execute skills/slash commands
pub struct SkillTool;

#[async_trait]
impl ToolHandler for SkillTool {
    fn description(&self) -> String {
        r#"Execute a skill within the main conversation

<skills_instructions>
When users ask you to perform tasks, check if any of the available skills below can help complete the task more effectively. Skills provide specialized capabilities and domain knowledge.

When users ask you to run a "slash command" or reference "/<something>" (e.g., "/commit", "/review-pr"), they are referring to a skill. Use this tool to invoke the corresponding skill.

<example>
User: "run /commit"
Assistant: [Calls Skill tool with skill: "commit"]
</example>

How to invoke:
- Use this tool with the skill name and optional arguments
- Examples:
  - `skill: "pdf"` - invoke the pdf skill
  - `skill: "commit", args: "-m 'Fix bug'"` - invoke with arguments
  - `skill: "review-pr", args: "123"` - invoke with arguments
  - `skill: "ms-office-suite:pdf"` - invoke using fully qualified name

Important:
- When a skill is relevant, you must invoke this tool IMMEDIATELY as your first action
- NEVER just announce or mention a skill in your text response without actually calling this tool
- This is a BLOCKING REQUIREMENT: invoke the relevant Skill tool BEFORE generating any other response about the task
- Only use skills listed in <available_skills> below
- Do not invoke a skill that is already running
- Do not use this tool for built-in CLI commands (like /help, /clear, etc.)
</skills_instructions>

<available_skills>
(Skills will be dynamically populated when available)
</available_skills>
"#
        .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The skill name. E.g., \"commit\", \"review-pr\", or \"pdf\""
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill"
                }
            },
            "required": ["skill"]
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let skill = input["skill"].as_str().unwrap_or("unknown");
        format!("Execute skill: {}", skill)
    }

    fn permission_details(&self, input: &Value) -> String {
        let skill = input["skill"].as_str().unwrap_or("unknown");
        let args = input["args"].as_str().unwrap_or("");
        if args.is_empty() {
            format!("Skill: {}", skill)
        } else {
            format!("Skill: {} with args: {}", skill, args)
        }
    }

    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Get skill name from input
        let skill_name = input["skill"]
            .as_str()
            .ok_or_else(|| Error::InvalidInput("Missing 'skill' field".to_string()))?
            .trim();

        if skill_name.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Invalid skill format: {}",
                input["skill"].as_str().unwrap_or("")
            )));
        }

        // Normalize skill name (remove leading /)
        let normalized_name = skill_name.strip_prefix('/').unwrap_or(skill_name);

        // Get optional arguments
        let args = input["args"].as_str().map(|s| s.to_string());

        // Get skill manager and look up the skill
        let manager = get_skill_manager().await.read().await;

        // Check if skill exists
        if !manager.skill_exists(normalized_name) {
            return Err(Error::NotFound(format!(
                "Unknown skill: {}",
                normalized_name
            )));
        }

        // Get the skill
        let skill = manager.get_skill(normalized_name).ok_or_else(|| {
            Error::NotFound(format!("Could not load skill: {}", normalized_name))
        })?;

        // Check if skill can be invoked via model
        if skill.disable_model_invocation {
            return Err(Error::PermissionDenied(format!(
                "Skill {} cannot be used with Skill tool due to disable-model-invocation",
                normalized_name
            )));
        }

        // Check if skill is prompt-based
        if skill.skill_type != SkillType::Prompt {
            return Err(Error::InvalidInput(format!(
                "Skill {} is not a prompt-based skill",
                normalized_name
            )));
        }

        // Get the prompt for this skill
        let prompt = skill.get_prompt_for_command(args.as_deref())?;

        // Build the result
        let result = SkillResult {
            success: true,
            command_name: normalized_name.to_string(),
            allowed_tools: skill.allowed_tools.clone(),
            model: skill.model.clone(),
        };

        // Return the result as JSON with the prompt included
        let output = json!({
            "result": result,
            "prompt": prompt,
            "skill_source": skill.source.as_str(),
        });

        Ok(serde_json::to_string_pretty(&output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_skills() {
        let skills = get_builtin_skills();
        assert!(skills.contains_key("commit"));
        assert!(skills.contains_key("review-pr"));
        assert!(skills.contains_key("pdf"));
    }

    #[test]
    fn test_skill_prompt_generation() {
        let skills = get_builtin_skills();
        let commit_skill = skills.get("commit").expect("commit skill should exist");

        let prompt = commit_skill
            .get_prompt_for_command(Some("fix: update readme"))
            .expect("should generate prompt");

        assert!(prompt.contains("ARGUMENTS: fix: update readme"));
    }

    #[test]
    fn test_skill_prompt_with_placeholder() {
        let skills = get_builtin_skills();
        let pdf_skill = skills.get("pdf").expect("pdf skill should exist");

        let prompt = pdf_skill
            .get_prompt_for_command(Some("/path/to/file.pdf"))
            .expect("should generate prompt");

        // $ARGUMENTS should be replaced
        assert!(!prompt.contains("$ARGUMENTS"));
        assert!(prompt.contains("/path/to/file.pdf"));
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: test-skill\ndescription: A test skill\nallowed-tools:\n  - Bash\n  - Read\n---\nThis is the skill content.\n";

        let (fm, body) = parse_skill_frontmatter(content);
        assert_eq!(fm.name, Some("test-skill".to_string()));
        assert_eq!(fm.description, Some("A test skill".to_string()));
        assert_eq!(
            fm.allowed_tools,
            Some(vec!["Bash".to_string(), "Read".to_string()])
        );
        // Body may have trailing whitespace from content, that's fine
        assert!(body.contains("This is the skill content."));
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "Just some content without frontmatter.";

        let (fm, body) = parse_skill_frontmatter(content);
        assert!(fm.name.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_skill_manager_new_has_builtins() {
        let manager = SkillManager::new();
        // New manager should have built-in skills
        assert!(manager.skill_exists("commit"));
        assert!(manager.skill_exists("review-pr"));
        assert!(manager.skill_exists("pdf"));
        assert!(!manager.skill_exists("nonexistent"));
    }

    #[test]
    fn test_skill_tool_description() {
        let tool = SkillTool;
        let desc = tool.description();
        assert!(desc.contains("skills_instructions"));
        assert!(desc.contains("available_skills"));
    }

    #[test]
    fn test_skill_tool_input_schema() {
        let tool = SkillTool;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["skill"].is_object());
        assert!(schema["properties"]["args"].is_object());
        let required = schema["required"].as_array().expect("required should be array");
        assert!(required.contains(&json!("skill")));
    }

    #[test]
    fn test_skill_result_serialization() {
        let result = SkillResult {
            success: true,
            command_name: "commit".to_string(),
            allowed_tools: Some(vec!["Bash".to_string()]),
            model: None,
        };
        let json_str = serde_json::to_string(&result).expect("should serialize");
        assert!(json_str.contains("\"success\":true"));
        assert!(json_str.contains("\"commandName\":\"commit\""));
        assert!(json_str.contains("\"allowedTools\""));
    }

    #[test]
    fn test_skill_source_as_str() {
        assert_eq!(SkillSource::BuiltIn.as_str(), "built-in");
        assert_eq!(SkillSource::UserSettings.as_str(), "userSettings");
        assert_eq!(SkillSource::ProjectSettings.as_str(), "projectSettings");
        assert_eq!(SkillSource::PolicySettings.as_str(), "policySettings");
    }

    #[test]
    fn test_skill_user_facing_name() {
        let skill = Skill {
            skill_type: SkillType::Prompt,
            name: "test-skill".to_string(),
            description: "Test".to_string(),
            allowed_tools: None,
            argument_hint: None,
            when_to_use: None,
            version: None,
            model: None,
            is_skill: true,
            disable_model_invocation: false,
            is_hidden: false,
            source: SkillSource::BuiltIn,
            skill_dir: None,
            content: Some("Test content".to_string()),
        };
        assert_eq!(skill.user_facing_name(), "test-skill");
    }

    #[test]
    fn test_format_available_skills() {
        let manager = SkillManager::new();
        let formatted = manager.format_available_skills();
        // Built-in skills should be in the output
        assert!(formatted.contains("/commit"));
        assert!(formatted.contains("/review-pr"));
        assert!(formatted.contains("/pdf"));
    }
}
