//! AskUserQuestion tool implementation
//!
//! This tool allows Claude to ask the user multiple choice questions to gather information,
//! clarify ambiguity, understand preferences, make decisions, or offer choices.
//!
//! Matches JavaScript implementation from cli-jsdef-fixed.js (around line 438296)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

use crate::ai::tools::ToolHandler;
use crate::error::{Error, Result};

/// Maximum length for header/chip text
const MAX_HEADER_LENGTH: usize = 12;

/// Option schema for a question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// The display text for this option that the user will see and select.
    /// Should be concise (1-5 words) and clearly describe the choice.
    pub label: String,
    /// Explanation of what this option means or what will happen if chosen.
    /// Useful for providing context about trade-offs or implications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Question schema
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// The complete question to ask the user. Should be clear, specific, and end with a question mark.
    /// Example: "Which library should we use for date formatting?"
    pub question: String,
    /// Very short label displayed as a chip/tag (max 12 chars).
    /// Examples: "Auth method", "Library", "Approach"
    pub header: String,
    /// The available choices for this question. Must have 2-4 options.
    /// Each option should be a distinct, mutually exclusive choice (unless multiSelect is enabled).
    /// There should be no 'Other' option, that will be provided automatically.
    pub options: Vec<QuestionOption>,
    /// Set to true to allow the user to select multiple options instead of just one.
    /// Use when choices are not mutually exclusive.
    #[serde(default)]
    pub multi_select: bool,
}

/// Input for AskUserQuestion tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestionInput {
    /// Questions to ask the user (1-4 questions)
    pub questions: Vec<Question>,
    /// User answers collected by the permission component (optional, filled by UI)
    #[serde(default)]
    pub answers: HashMap<String, String>,
}

/// Output for AskUserQuestion tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestionOutput {
    /// The questions that were asked
    pub questions: Vec<Question>,
    /// The answers provided by the user (question text -> answer string; multi-select answers are comma-separated)
    pub answers: HashMap<String, String>,
}

/// AskUserQuestion tool - matches JavaScript implementation
///
/// This tool is used when Claude needs to ask the user questions during execution.
/// It allows:
/// 1. Gathering user preferences or requirements
/// 2. Clarifying ambiguous instructions
/// 3. Getting decisions on implementation choices as work progresses
/// 4. Offering choices to the user about what direction to take
///
/// Users will always be able to select "Other" to provide custom text input.
pub struct AskUserQuestionTool;

#[async_trait]
impl ToolHandler for AskUserQuestionTool {
    fn description(&self) -> String {
        "Asks the user multiple choice questions to gather information, clarify ambiguity, understand preferences, make decisions or offer them choices.".to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to ask the user (1-4 questions)",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The complete question to ask the user. Should be clear, specific, and end with a question mark. Example: \"Which library should we use for date formatting?\" If multiSelect is true, phrase it accordingly, e.g. \"Which features do you want to enable?\""
                            },
                            "header": {
                                "type": "string",
                                "description": format!("Very short label displayed as a chip/tag (max {} chars). Examples: \"Auth method\", \"Library\", \"Approach\".", MAX_HEADER_LENGTH),
                                "maxLength": MAX_HEADER_LENGTH
                            },
                            "options": {
                                "type": "array",
                                "description": "The available choices for this question. Must have 2-4 options. Each option should be a distinct, mutually exclusive choice (unless multiSelect is enabled). There should be no 'Other' option, that will be provided automatically.",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "The display text for this option that the user will see and select. Should be concise (1-5 words) and clearly describe the choice."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Explanation of what this option means or what will happen if chosen. Useful for providing context about trade-offs or implications."
                                        }
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "description": "Set to true to allow the user to select multiple options instead of just one. Use when choices are not mutually exclusive."
                            }
                        },
                        "required": ["question", "header", "options"]
                    }
                },
                "answers": {
                    "type": "object",
                    "description": "User answers collected by the permission component",
                    "additionalProperties": {
                        "type": "string"
                    }
                }
            },
            "required": ["questions"],
            "additionalProperties": false
        })
    }

    fn action_description(&self, input: &Value) -> String {
        let question_count = input["questions"]
            .as_array()
            .map(|arr| arr.len())
            .unwrap_or(0);

        if question_count == 1 {
            "Ask user 1 question".to_string()
        } else {
            format!("Ask user {} questions", question_count)
        }
    }

    fn permission_details(&self, _input: &Value) -> String {
        "Answer questions?".to_string()
    }

    async fn execute(&self, input: Value, _cancellation_token: Option<CancellationToken>) -> Result<String> {
        // Parse input
        let parsed_input: AskUserQuestionInput = serde_json::from_value(input.clone())
            .map_err(|e| Error::InvalidInput(format!("Invalid AskUserQuestion input: {}", e)))?;

        // Validate questions
        if parsed_input.questions.is_empty() {
            return Err(Error::InvalidInput("At least one question is required".to_string()));
        }
        if parsed_input.questions.len() > 4 {
            return Err(Error::InvalidInput("Maximum of 4 questions allowed".to_string()));
        }

        // Validate each question
        for question in &parsed_input.questions {
            // Check header length
            if question.header.len() > MAX_HEADER_LENGTH {
                return Err(Error::InvalidInput(format!(
                    "Header '{}' exceeds maximum length of {} characters",
                    question.header, MAX_HEADER_LENGTH
                )));
            }

            // Check options count
            if question.options.len() < 2 {
                return Err(Error::InvalidInput(format!(
                    "Question '{}' must have at least 2 options",
                    question.question
                )));
            }
            if question.options.len() > 4 {
                return Err(Error::InvalidInput(format!(
                    "Question '{}' must have at most 4 options",
                    question.question
                )));
            }

            // Check for duplicate option labels
            let labels: Vec<&str> = question.options.iter().map(|o| o.label.as_str()).collect();
            let unique_labels: std::collections::HashSet<&str> = labels.iter().cloned().collect();
            if labels.len() != unique_labels.len() {
                return Err(Error::InvalidInput(format!(
                    "Question '{}' has duplicate option labels",
                    question.question
                )));
            }
        }

        // Check for duplicate questions
        let question_texts: Vec<&str> = parsed_input.questions.iter().map(|q| q.question.as_str()).collect();
        let unique_questions: std::collections::HashSet<&str> = question_texts.iter().cloned().collect();
        if question_texts.len() != unique_questions.len() {
            return Err(Error::InvalidInput("Question texts must be unique".to_string()));
        }

        // Build output - the answers come from the UI through the permission component
        let output = AskUserQuestionOutput {
            questions: parsed_input.questions,
            answers: parsed_input.answers,
        };

        // Return JSON output matching JavaScript structure
        let result = serde_json::to_string(&output)
            .map_err(|e| Error::Serialization(e))?;

        Ok(result)
    }
}

/// Format the tool result for the model (matches JavaScript mapToolResultToToolResultBlockParam)
pub fn format_tool_result(answers: &HashMap<String, String>, tool_use_id: &str) -> Value {
    let answers_str = answers
        .iter()
        .map(|(question, answer)| format!("\"{}\"=\"{}\"", question, answer))
        .collect::<Vec<_>>()
        .join(", ");

    json!({
        "type": "tool_result",
        "content": format!(
            "User has answered your questions: {}. You can now continue with the user's answers in mind.",
            answers_str
        ),
        "tool_use_id": tool_use_id
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ask_user_question_basic() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": [{
                "question": "Which library should we use for date formatting?",
                "header": "Library",
                "options": [
                    {"label": "date-fns", "description": "Modern, tree-shakable"},
                    {"label": "moment.js", "description": "Full-featured but large"}
                ]
            }],
            "answers": {
                "Which library should we use for date formatting?": "date-fns"
            }
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_ok());

        let output: AskUserQuestionOutput = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(output.questions.len(), 1);
        assert_eq!(output.answers.len(), 1);
    }

    #[tokio::test]
    async fn test_ask_user_question_multi_select() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": [{
                "question": "Which features do you want to enable?",
                "header": "Features",
                "options": [
                    {"label": "Dark mode"},
                    {"label": "Notifications"},
                    {"label": "Analytics"}
                ],
                "multiSelect": true
            }],
            "answers": {
                "Which features do you want to enable?": "Dark mode, Notifications"
            }
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ask_user_question_validation_no_questions() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": []
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ask_user_question_validation_too_many_questions() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": [
                {"question": "Q1?", "header": "H1", "options": [{"label": "A"}, {"label": "B"}]},
                {"question": "Q2?", "header": "H2", "options": [{"label": "A"}, {"label": "B"}]},
                {"question": "Q3?", "header": "H3", "options": [{"label": "A"}, {"label": "B"}]},
                {"question": "Q4?", "header": "H4", "options": [{"label": "A"}, {"label": "B"}]},
                {"question": "Q5?", "header": "H5", "options": [{"label": "A"}, {"label": "B"}]}
            ]
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ask_user_question_validation_header_too_long() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": [{
                "question": "Test question?",
                "header": "This header is way too long for the chip",
                "options": [{"label": "A"}, {"label": "B"}]
            }]
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ask_user_question_validation_duplicate_labels() {
        let tool = AskUserQuestionTool;

        let input = json!({
            "questions": [{
                "question": "Test question?",
                "header": "Test",
                "options": [{"label": "Same"}, {"label": "Same"}]
            }]
        });

        let result = tool.execute(input, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_format_tool_result() {
        let mut answers = HashMap::new();
        answers.insert("Which library?".to_string(), "React".to_string());
        answers.insert("Which style?".to_string(), "CSS Modules".to_string());

        let result = format_tool_result(&answers, "test-id");

        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "test-id");
        assert!(result["content"].as_str().unwrap().contains("User has answered your questions"));
    }
}
