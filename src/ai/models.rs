use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub aliases: Vec<String>,
}

/// Model capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub text_generation: bool,
    pub code_generation: bool,
    pub reasoning: bool,
    pub analysis: bool,
    pub creative_writing: bool,
    pub conversation: bool,
    pub instruction_following: bool,
}

/// Model presets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreset {
    pub name: String,
    pub description: String,
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<u32>,
}

/// Model registry
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
    aliases: HashMap<String, String>,
    presets: HashMap<String, ModelPreset>,
}

impl ModelRegistry {
    /// Create a new model registry
    pub fn new() -> Self {
        let mut registry = Self {
            models: HashMap::new(),
            aliases: HashMap::new(),
            presets: HashMap::new(),
        };
        
        registry.register_default_models();
        registry.register_default_presets();
        
        registry
    }
    
    /// Register default models
    fn register_default_models(&mut self) {
        // Claude 4 Opus (Latest)
        self.register_model(ModelInfo {
            id: "claude-opus-4-1-20250805".to_string(),
            name: "Claude 4.1 Opus".to_string(),
            description: "Most capable model for complex tasks".to_string(),
            context_window: 200000,
            max_output_tokens: 4096,
            input_cost_per_1k: 0.015,
            output_cost_per_1k: 0.075,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["opus".to_string(), "claude-4-opus".to_string(), "opus-4-1".to_string()],
        });
        
        // Claude 4 Opus (Previous)
        self.register_model(ModelInfo {
            id: "claude-opus-4-20250514".to_string(),
            name: "Claude 4 Opus".to_string(),
            description: "Balanced model for most tasks".to_string(),
            context_window: 200000,
            max_output_tokens: 4096,
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["opus-4".to_string(), "claude-opus-4".to_string()],
        });
        
        // Claude 3 Haiku
        self.register_model(ModelInfo {
            id: "claude-3-haiku-20240307".to_string(),
            name: "Claude 3 Haiku".to_string(),
            description: "Fast and efficient model".to_string(),
            context_window: 200000,
            max_output_tokens: 4096,
            input_cost_per_1k: 0.00025,
            output_cost_per_1k: 0.00125,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["haiku".to_string(), "claude-3-haiku".to_string()],
        });
        
        // Claude 3.5 Sonnet
        self.register_model(ModelInfo {
            id: "claude-3-5-sonnet-20241022".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            description: "Latest and most capable Sonnet model".to_string(),
            context_window: 200000,
            max_output_tokens: 8192,
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![
                "claude-3.5-sonnet".to_string(),
                "sonnet-3.5".to_string(),
                "latest".to_string(),
            ],
        });
    }
    
    /// Register default presets
    fn register_default_presets(&mut self) {
        // Coding preset
        self.register_preset(ModelPreset {
            name: "coding".to_string(),
            description: "Optimized for code generation".to_string(),
            temperature: 0.0,
            top_p: Some(0.95),
            top_k: None,
            max_tokens: Some(4096),
        });
        
        // Creative writing preset
        self.register_preset(ModelPreset {
            name: "creative".to_string(),
            description: "Optimized for creative writing".to_string(),
            temperature: 0.9,
            top_p: Some(0.95),
            top_k: Some(50),
            max_tokens: None,
        });
        
        // Analysis preset
        self.register_preset(ModelPreset {
            name: "analysis".to_string(),
            description: "Optimized for analysis and reasoning".to_string(),
            temperature: 0.2,
            top_p: Some(0.95),
            top_k: None,
            max_tokens: None,
        });
        
        // Conversation preset
        self.register_preset(ModelPreset {
            name: "conversation".to_string(),
            description: "Optimized for conversational responses".to_string(),
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: None,
            max_tokens: None,
        });
        
        // Precise preset
        self.register_preset(ModelPreset {
            name: "precise".to_string(),
            description: "Most deterministic responses".to_string(),
            temperature: 0.0,
            top_p: Some(1.0),
            top_k: Some(1),
            max_tokens: None,
        });
    }
    
    /// Register a model
    pub fn register_model(&mut self, model: ModelInfo) {
        // Register aliases
        for alias in &model.aliases {
            self.aliases.insert(alias.clone(), model.id.clone());
        }
        
        self.models.insert(model.id.clone(), model);
    }
    
    /// Register a preset
    pub fn register_preset(&mut self, preset: ModelPreset) {
        self.presets.insert(preset.name.clone(), preset);
    }
    
    /// Get a model by ID or alias
    pub fn get_model(&self, id_or_alias: &str) -> Option<&ModelInfo> {
        // Try direct lookup
        if let Some(model) = self.models.get(id_or_alias) {
            return Some(model);
        }
        
        // Try alias lookup
        if let Some(model_id) = self.aliases.get(id_or_alias) {
            return self.models.get(model_id);
        }
        
        None
    }
    
    /// Get a preset by name
    pub fn get_preset(&self, name: &str) -> Option<&ModelPreset> {
        self.presets.get(name)
    }
    
    /// List all models
    pub fn list_models(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }
    
    /// List all presets
    pub fn list_presets(&self) -> Vec<&ModelPreset> {
        self.presets.values().collect()
    }
    
    /// Resolve model ID from alias
    pub fn resolve_model_id(&self, id_or_alias: &str) -> Option<String> {
        if self.models.contains_key(id_or_alias) {
            Some(id_or_alias.to_string())
        } else {
            self.aliases.get(id_or_alias).cloned()
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Model selector for choosing the best model
pub struct ModelSelector {
    registry: ModelRegistry,
}

impl ModelSelector {
    /// Create a new model selector
    pub fn new(registry: ModelRegistry) -> Self {
        Self { registry }
    }
    
    /// Select model based on requirements
    pub fn select_model(&self, requirements: &ModelRequirements) -> Option<&ModelInfo> {
        let mut candidates: Vec<_> = self.registry.list_models();
        
        // Filter by context window
        if let Some(min_context) = requirements.min_context_window {
            candidates.retain(|m| m.context_window >= min_context);
        }
        
        // Filter by max output tokens
        if let Some(min_output) = requirements.min_output_tokens {
            candidates.retain(|m| m.max_output_tokens >= min_output);
        }
        
        // Filter by capabilities
        if requirements.requires_tools {
            candidates.retain(|m| m.supports_tools);
        }
        
        if requirements.requires_vision {
            candidates.retain(|m| m.supports_vision);
        }
        
        if requirements.requires_streaming {
            candidates.retain(|m| m.supports_streaming);
        }
        
        // Sort by cost if budget conscious
        if requirements.optimize_for_cost {
            candidates.sort_by(|a, b| {
                let a_cost = a.input_cost_per_1k + a.output_cost_per_1k;
                let b_cost = b.input_cost_per_1k + b.output_cost_per_1k;
                a_cost.partial_cmp(&b_cost).unwrap()
            });
        } else {
            // Sort by capability (context window as proxy)
            candidates.sort_by(|a, b| b.context_window.cmp(&a.context_window));
        }
        
        candidates.first().copied()
    }
}

/// Model requirements for selection
#[derive(Debug, Clone)]
pub struct ModelRequirements {
    pub min_context_window: Option<u32>,
    pub min_output_tokens: Option<u32>,
    pub requires_tools: bool,
    pub requires_vision: bool,
    pub requires_streaming: bool,
    pub optimize_for_cost: bool,
}

impl Default for ModelRequirements {
    fn default() -> Self {
        Self {
            min_context_window: None,
            min_output_tokens: None,
            requires_tools: false,
            requires_vision: false,
            requires_streaming: false,
            optimize_for_cost: false,
        }
    }
}

/// Model usage tracker
pub struct ModelUsageTracker {
    usage: HashMap<String, ModelUsage>,
}

/// Model usage statistics
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub error_count: u64,
}

impl ModelUsageTracker {
    /// Create a new usage tracker
    pub fn new() -> Self {
        Self {
            usage: HashMap::new(),
        }
    }
    
    /// Track a request
    pub fn track_request(
        &mut self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u64,
        model_info: &ModelInfo,
        error: bool,
    ) {
        let usage = self.usage.entry(model_id.to_string()).or_default();
        
        usage.total_requests += 1;
        usage.total_input_tokens += input_tokens as u64;
        usage.total_output_tokens += output_tokens as u64;
        usage.total_duration_ms += duration_ms;
        
        // Calculate cost
        let input_cost = (input_tokens as f64 / 1000.0) * model_info.input_cost_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * model_info.output_cost_per_1k;
        usage.total_cost += input_cost + output_cost;
        
        if error {
            usage.error_count += 1;
        }
    }
    
    /// Get usage for a model
    pub fn get_usage(&self, model_id: &str) -> Option<&ModelUsage> {
        self.usage.get(model_id)
    }
    
    /// Get total usage across all models
    pub fn get_total_usage(&self) -> ModelUsage {
        let mut total = ModelUsage::default();
        
        for usage in self.usage.values() {
            total.total_requests += usage.total_requests;
            total.total_input_tokens += usage.total_input_tokens;
            total.total_output_tokens += usage.total_output_tokens;
            total.total_cost += usage.total_cost;
            total.total_duration_ms += usage.total_duration_ms;
            total.error_count += usage.error_count;
        }
        
        total
    }
    
    /// Clear usage statistics
    pub fn clear(&mut self) {
        self.usage.clear();
    }
}