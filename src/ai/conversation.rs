use crate::ai::{
    client_adapter::AIClientAdapter, ContentPart, Message, MessageContent, MessageRole, Tool, ToolChoice,
};
use crate::error::{Error, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Conversation manager
pub struct ConversationManager {
    conversations: RwLock<HashMap<String, Conversation>>,
    storage_dir: PathBuf,
    auto_save: bool,
}

impl ConversationManager {
    /// Create a new conversation manager
    pub fn new(storage_dir: PathBuf, auto_save: bool) -> Result<Self> {
        fs::create_dir_all(&storage_dir)?;
        
        let mut conversations = HashMap::new();
        
        // Load existing conversations
        if let Ok(entries) = fs::read_dir(&storage_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let id = name.trim_end_matches(".json");
                        let path = entry.path();
                        
                        match load_conversation(&path) {
                            Ok(conv) => {
                                conversations.insert(id.to_string(), conv);
                            }
                            Err(e) => {
                                eprintln!("Failed to load conversation {}: {}", id, e);
                            }
                        }
                    }
                }
            }
        }
        
        Ok(Self {
            conversations: RwLock::new(conversations),
            storage_dir,
            auto_save,
        })
    }
    
    /// Create a new conversation
    pub async fn create_conversation(
        &self,
        model: String,
        system_prompt: Option<String>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let conversation = Conversation::new(id.clone(), model, system_prompt);
        
        if self.auto_save {
            self.save_conversation(&conversation).await?;
        }
        
        self.conversations.write().await.insert(id.clone(), conversation);
        
        Ok(id)
    }
    
    /// Get a conversation
    pub async fn get_conversation(&self, id: &str) -> Result<Conversation> {
        self.conversations
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("Conversation {} not found", id)))
    }
    
    /// List all conversations
    pub async fn list_conversations(&self) -> Vec<ConversationInfo> {
        self.conversations
            .read()
            .await
            .values()
            .map(|conv| ConversationInfo {
                id: conv.id.clone(),
                title: conv.title.clone(),
                model: conv.model.clone(),
                created_at: conv.created_at,
                updated_at: conv.updated_at,
                message_count: conv.messages.len(),
            })
            .collect()
    }
    
    /// Add a message to a conversation
    pub async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: String,
    ) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| Error::NotFound(format!("Conversation {} not found", conversation_id)))?;
        
        conversation.add_message(role, MessageContent::Text(content));
        
        if self.auto_save {
            self.save_conversation(conversation).await?;
        }
        
        Ok(())
    }
    
    /// Get AI response for a conversation
    pub async fn get_response(
        &self,
        client: &AIClientAdapter,
        conversation_id: &str,
        tools: Option<Vec<Tool>>,
    ) -> Result<String> {
        let mut conversations = self.conversations.write().await;
        
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| Error::NotFound(format!("Conversation {} not found", conversation_id)))?;
        
        // Build request
        let mut request = client
            .create_chat_request()
            .messages(conversation.get_messages_for_api())
            .max_tokens(4096);
        
        if let Some(system) = &conversation.system_prompt {
            request = request.system(system.clone());
        }
        
        if let Some(tools) = tools {
            request = request.tools(tools).tool_choice(ToolChoice::Auto);
        }
        
        let response = client.chat(request.build()).await?;
        
        // Extract text content
        let mut text_content = String::new();
        for part in &response.content {
            match part {
                ContentPart::Text { text, .. } => {
                    text_content.push_str(text);
                }
                ContentPart::ToolUse { name, .. } => {
                    text_content.push_str(&format!("\n[Tool: {}]\n", name));
                }
                _ => {}
            }
        }
        
        // Update conversation metadata first (before moving content)
        conversation.update_from_response(&response);
        
        // Add assistant message (moves response.content)
        conversation.add_message(MessageRole::Assistant, MessageContent::Multipart(response.content));
        
        if self.auto_save {
            self.save_conversation(conversation).await?;
        }
        
        Ok(text_content)
    }
    
    /// Save a conversation
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        let path = self.storage_dir.join(format!("{}.json", conversation.id));
        let json = serde_json::to_string_pretty(conversation)?;
        fs::write(path, json)?;
        Ok(())
    }
    
    /// Delete a conversation
    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        self.conversations.write().await.remove(id);
        
        let path = self.storage_dir.join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
        }
        
        Ok(())
    }
    
    /// Clear all conversations
    pub async fn clear_all(&self) -> Result<()> {
        self.conversations.write().await.clear();
        
        // Remove all files
        if let Ok(entries) = fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: HashMap<String, String>,
    pub token_usage: TokenUsage,
}

impl Conversation {
    /// Create a new conversation
    pub fn new(id: String, model: String, system_prompt: Option<String>) -> Self {
        let now = crate::utils::timestamp_ms();
        
        Self {
            id,
            title: None,
            model,
            system_prompt,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            token_usage: TokenUsage::default(),
        }
    }
    
    /// Add a message
    pub fn add_message(&mut self, role: MessageRole, content: MessageContent) {
        // Auto-generate title from first user message before moving content
        if self.title.is_none() && role == MessageRole::User {
            if let MessageContent::Text(text) = &content {
                self.title = Some(generate_title(text));
            }
        }
        
        self.messages.push(Message {
            role,
            content,
            name: None,
        });
        self.updated_at = crate::utils::timestamp_ms();
    }
    
    /// Get messages formatted for API
    pub fn get_messages_for_api(&self) -> Vec<Message> {
        self.messages.clone()
    }
    
    /// Update from API response
    pub fn update_from_response(&mut self, response: &crate::ai::ChatResponse) {
        self.token_usage.input_tokens += response.usage.input_tokens;
        self.token_usage.output_tokens += response.usage.output_tokens;
        self.updated_at = crate::utils::timestamp_ms();
    }
    
    /// Get total message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
    
    /// Get last user message
    pub fn last_user_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
    }
    
    /// Get last assistant message
    pub fn last_assistant_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }
    
    /// Clear messages
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.token_usage = TokenUsage::default();
        self.updated_at = crate::utils::timestamp_ms();
    }
}

/// Token usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl TokenUsage {
    /// Get total tokens
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

/// Conversation info for listing
#[derive(Debug, Clone, Serialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
}

/// Load conversation from file
fn load_conversation(path: &Path) -> Result<Conversation> {
    let json = fs::read_to_string(path)?;
    let conversation = serde_json::from_str(&json)?;
    Ok(conversation)
}

/// Generate title from first message
fn generate_title(text: &str) -> String {
    let max_len = 50;
    let cleaned = text
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();
    
    if cleaned.len() <= max_len {
        cleaned
    } else {
        format!("{}...", &cleaned[..max_len - 3])
    }
}