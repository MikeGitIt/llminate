use crate::ai::{
    client::{ContentBlock, ContentDelta},
    ContentPart, MessageRole,
};

// Re-export types for external use
pub use crate::ai::client::{StreamEvent, ContentDelta as StreamDelta};
use crate::error::{Error, Result};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc;

/// Streaming response handler
pub struct StreamingHandler {
    tx: mpsc::UnboundedSender<StreamingUpdate>,
    rx: mpsc::UnboundedReceiver<StreamingUpdate>,
}

/// Streaming update event
#[derive(Debug, Clone)]
pub enum StreamingUpdate {
    /// Text chunk received
    TextChunk(String),
    /// Tool use started
    ToolUseStart {
        id: String,
        name: String,
    },
    /// Tool input chunk
    ToolInputChunk {
        id: String,
        chunk: String,
    },
    /// Tool use completed
    ToolUseComplete {
        id: String,
        input: serde_json::Value,
    },
    /// Thinking started (interleaved-thinking-2025-05-14 beta)
    ThinkingStart,
    /// Thinking chunk received
    ThinkingChunk(String),
    /// Thinking completed with signature
    ThinkingComplete {
        thinking: String,
        signature: Option<String>,
    },
    /// Message completed
    MessageComplete {
        stop_reason: Option<String>,
        usage: TokenUsage,
    },
    /// Error occurred
    Error(String),
}

/// Token usage for streaming
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl StreamingHandler {
    /// Create a new streaming handler that processes a stream with optional cancellation
    pub fn process_stream(
        stream: Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>,
        mut cancel_rx: Option<mpsc::UnboundedReceiver<()>>,
    ) -> (mpsc::UnboundedReceiver<StreamingUpdate>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded_channel();
        
        // Spawn the processing task and return both the receiver and join handle
        let handle = tokio::spawn(async move {
            let mut stream = stream;
            let mut current_tool_id = None;
            let mut tool_input_buffer = String::new();
            let mut total_usage = TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            };
            
            loop {
                // Check for cancellation or next stream event
                let next_event = if let Some(ref mut cancel) = cancel_rx {
                    tokio::select! {
                        _ = cancel.recv() => {
                            // Cancellation requested
                            let _ = tx.send(StreamingUpdate::Error("Stream cancelled by user".to_string()));
                            break;
                        }
                        event = stream.next() => event
                    }
                } else {
                    stream.next().await
                };
                
                match next_event {
                    Some(event_result) => {
                match event_result {
                    Ok(event) => {
                        match event {
                            StreamEvent::MessageStart { message } => {
                                total_usage.input_tokens = message.usage.input_tokens;
                            }
                            StreamEvent::ContentBlockStart { content_block, .. } => {
                                match content_block {
                                    ContentBlock::Text { text } => {
                                        let _ = tx.send(StreamingUpdate::TextChunk(text));
                                    }
                                    ContentBlock::ToolUse { id, name, .. } => {
                                        current_tool_id = Some(id.clone());
                                        tool_input_buffer.clear();
                                        let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
                                    }
                                    ContentBlock::Thinking { thinking, .. } => {
                                        let _ = tx.send(StreamingUpdate::ThinkingStart);
                                        if !thinking.is_empty() {
                                            let _ = tx.send(StreamingUpdate::ThinkingChunk(thinking));
                                        }
                                    }
                                    ContentBlock::RedactedThinking { .. } => {
                                        // Redacted thinking is not displayed to user
                                    }
                                }
                            }
                            StreamEvent::ContentBlockDelta { delta, .. } => {
                                match delta {
                                    ContentDelta::TextDelta { text } => {
                                        let _ = tx.send(StreamingUpdate::TextChunk(text));
                                    }
                                    ContentDelta::InputJsonDelta { partial_json } => {
                                        if let Some(id) = &current_tool_id {
                                            tool_input_buffer.push_str(&partial_json);
                                            let _ = tx.send(StreamingUpdate::ToolInputChunk {
                                                id: id.clone(),
                                                chunk: partial_json,
                                            });
                                        }
                                    }
                                    ContentDelta::ThinkingDelta { thinking } => {
                                        let _ = tx.send(StreamingUpdate::ThinkingChunk(thinking));
                                    }
                                    ContentDelta::SignatureDelta { .. } => {
                                        // Signature is internal, not displayed
                                    }
                                }
                            }
                            StreamEvent::ContentBlockStop { .. } => {
                                if let Some(id) = current_tool_id.take() {
                                    match serde_json::from_str(&tool_input_buffer) {
                                        Ok(input) => {
                                            let _ = tx.send(StreamingUpdate::ToolUseComplete {
                                                id,
                                                input,
                                            });
                                        }
                                        Err(e) => {
                                            let _ = tx.send(StreamingUpdate::Error(format!(
                                                "Failed to parse tool input: {}",
                                                e
                                            )));
                                        }
                                    }
                                    tool_input_buffer.clear();
                                }
                            }
                            StreamEvent::MessageDelta { usage, .. } => {
                                total_usage.output_tokens = usage.output_tokens;
                            }
                            StreamEvent::MessageStop => {
                                let _ = tx.send(StreamingUpdate::MessageComplete {
                                    stop_reason: None,
                                    usage: total_usage.clone(),
                                });
                                break;
                            }
                            StreamEvent::Ping => {
                                // Ignore ping events
                            }
                            StreamEvent::Error(error) => {
                                let _ = tx.send(StreamingUpdate::Error(error));
                                break;
                            }
                            // Handle new variants
                            StreamEvent::ContentStart { .. } => {}
                            StreamEvent::ContentDelta { .. } => {}
                            StreamEvent::ContentStop => {}
                            StreamEvent::ToolUseStart { id, name } => {
                                let _ = tx.send(StreamingUpdate::ToolUseStart { id, name });
                            }
                            StreamEvent::ToolUseDelta { .. } => {}
                            StreamEvent::ToolUseStop { id, input, .. } => {
                                let _ = tx.send(StreamingUpdate::ToolUseComplete { id, input });
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamingUpdate::Error(e.to_string()));
                        break;
                    }
                }
                    }
                    None => {
                        // Stream ended
                        break;
                    }
                }
            }
            
            // Send a final complete message if we haven't already
            let _ = tx.send(StreamingUpdate::MessageComplete {
                stop_reason: Some("stream_ended".to_string()),
                usage: total_usage,
            });
        });
        
        (rx, handle)
    }
}

/// Streaming response accumulator
pub struct StreamAccumulator {
    text_buffer: String,
    tool_uses: Vec<AccumulatedToolUse>,
    current_tool_index: Option<usize>,
    usage: TokenUsage,
    /// Accumulated thinking content (interleaved-thinking-2025-05-14 beta)
    thinking_buffer: String,
    /// Whether currently in thinking mode
    is_thinking: bool,
    /// Thinking signature (for verification)
    thinking_signature: Option<String>,
}

/// Accumulated tool use
#[derive(Debug, Clone)]
pub struct AccumulatedToolUse {
    pub id: String,
    pub name: String,
    pub input: Option<serde_json::Value>,
    pub input_buffer: String,
}

impl StreamAccumulator {
    /// Create a new accumulator
    pub fn new() -> Self {
        Self {
            text_buffer: String::new(),
            tool_uses: Vec::new(),
            current_tool_index: None,
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
            thinking_buffer: String::new(),
            is_thinking: false,
            thinking_signature: None,
        }
    }

    /// Process a streaming update
    pub fn process_update(&mut self, update: StreamingUpdate) {
        match update {
            StreamingUpdate::TextChunk(chunk) => {
                self.text_buffer.push_str(&chunk);
            }
            StreamingUpdate::ToolUseStart { id, name } => {
                self.tool_uses.push(AccumulatedToolUse {
                    id,
                    name,
                    input: None,
                    input_buffer: String::new(),
                });
                self.current_tool_index = Some(self.tool_uses.len() - 1);
            }
            StreamingUpdate::ToolInputChunk { chunk, .. } => {
                if let Some(index) = self.current_tool_index {
                    if let Some(tool) = self.tool_uses.get_mut(index) {
                        tool.input_buffer.push_str(&chunk);
                    }
                }
            }
            StreamingUpdate::ToolUseComplete { id, input } => {
                if let Some(tool) = self.tool_uses.iter_mut().find(|t| t.id == id) {
                    tool.input = Some(input);
                }
                self.current_tool_index = None;
            }
            StreamingUpdate::ThinkingStart => {
                self.is_thinking = true;
                self.thinking_buffer.clear();
            }
            StreamingUpdate::ThinkingChunk(chunk) => {
                self.thinking_buffer.push_str(&chunk);
            }
            StreamingUpdate::ThinkingComplete { thinking, signature } => {
                self.thinking_buffer = thinking;
                self.thinking_signature = signature;
                self.is_thinking = false;
            }
            StreamingUpdate::MessageComplete { usage, .. } => {
                self.usage = usage;
            }
            StreamingUpdate::Error(_) => {
                // Error handling done elsewhere
            }
        }
    }

    /// Check if currently in thinking mode
    pub fn is_thinking(&self) -> bool {
        self.is_thinking
    }

    /// Get accumulated thinking content
    pub fn get_thinking(&self) -> &str {
        &self.thinking_buffer
    }
    
    /// Get accumulated text
    pub fn get_text(&self) -> &str {
        &self.text_buffer
    }
    
    /// Get tool uses
    pub fn get_tool_uses(&self) -> &[AccumulatedToolUse] {
        &self.tool_uses
    }
    
    /// Get usage
    pub fn get_usage(&self) -> &TokenUsage {
        &self.usage
    }
    
    /// Convert to content parts
    pub fn to_content_parts(self) -> Vec<ContentPart> {
        let mut parts = Vec::new();
        
        if !self.text_buffer.is_empty() {
            parts.push(ContentPart::Text {
                text: self.text_buffer,
                citations: None,
            });
        }
        
        for tool in self.tool_uses {
            if let Some(input) = tool.input {
                parts.push(ContentPart::ToolUse {
                    id: tool.id,
                    name: tool.name,
                    input,
                });
            }
        }
        
        parts
    }
}

/// Stream processor for handling responses
pub struct StreamProcessor<F>
where
    F: Fn(StreamingUpdate) + Send + 'static,
{
    callback: F,
}

impl<F> StreamProcessor<F>
where
    F: Fn(StreamingUpdate) + Send + 'static,
{
    /// Create a new stream processor
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
    
    /// Process a stream with the callback
    pub async fn process(
        self,
        stream: Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>,
    ) -> Result<StreamAccumulator> {
        let mut accumulator = StreamAccumulator::new();
        let mut stream = stream;
        
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(event) => {
                    let update = match event {
                        StreamEvent::MessageStart { message } => {
                            accumulator.usage.input_tokens = message.usage.input_tokens;
                            continue;
                        }
                        StreamEvent::ContentBlockStart { content_block, .. } => {
                            match content_block {
                                ContentBlock::Text { text } => StreamingUpdate::TextChunk(text),
                                ContentBlock::ToolUse { id, name, .. } => {
                                    StreamingUpdate::ToolUseStart { id, name }
                                }
                                ContentBlock::Thinking { thinking, .. } => {
                                    if thinking.is_empty() {
                                        StreamingUpdate::ThinkingStart
                                    } else {
                                        StreamingUpdate::ThinkingChunk(thinking)
                                    }
                                }
                                ContentBlock::RedactedThinking { .. } => {
                                    continue; // Redacted thinking not shown to user
                                }
                            }
                        }
                        StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                            ContentDelta::TextDelta { text } => StreamingUpdate::TextChunk(text),
                            ContentDelta::InputJsonDelta { partial_json } => {
                                if let Some(index) = accumulator.current_tool_index {
                                    if let Some(tool) = accumulator.tool_uses.get(index) {
                                        StreamingUpdate::ToolInputChunk {
                                            id: tool.id.clone(),
                                            chunk: partial_json,
                                        }
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                }
                            }
                            ContentDelta::ThinkingDelta { thinking } => {
                                StreamingUpdate::ThinkingChunk(thinking)
                            }
                            ContentDelta::SignatureDelta { .. } => {
                                continue; // Signature is internal
                            }
                        },
                        StreamEvent::ContentBlockStop { .. } => {
                            if let Some(index) = accumulator.current_tool_index {
                                if let Some(tool) = accumulator.tool_uses.get_mut(index) {
                                    match serde_json::from_str(&tool.input_buffer) {
                                        Ok(input) => StreamingUpdate::ToolUseComplete {
                                            id: tool.id.clone(),
                                            input,
                                        },
                                        Err(e) => StreamingUpdate::Error(format!(
                                            "Failed to parse tool input: {}",
                                            e
                                        )),
                                    }
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        }
                        StreamEvent::MessageDelta { usage, delta } => {
                            accumulator.usage.output_tokens = usage.output_tokens;
                            StreamingUpdate::MessageComplete {
                                stop_reason: delta.stop_reason.map(|r| format!("{:?}", r)),
                                usage: accumulator.usage.clone(),
                            }
                        }
                        StreamEvent::MessageStop => {
                            let update = StreamingUpdate::MessageComplete {
                                stop_reason: None,
                                usage: accumulator.usage.clone(),
                            };
                            accumulator.process_update(update.clone());
                            (self.callback)(update);
                            break;
                        }
                        StreamEvent::Ping => continue,
                        StreamEvent::Error(error) => StreamingUpdate::Error(error),
                        // Handle new variants
                        StreamEvent::ContentStart { .. } => continue,
                        StreamEvent::ContentDelta { .. } => continue,
                        StreamEvent::ContentStop => continue,
                        StreamEvent::ToolUseStart { id, name } => {
                            StreamingUpdate::ToolUseStart { id, name }
                        }
                        StreamEvent::ToolUseDelta { .. } => continue,
                        StreamEvent::ToolUseStop { id, input, .. } => {
                            StreamingUpdate::ToolUseComplete { id, input }
                        }
                    };
                    
                    accumulator.process_update(update.clone());
                    (self.callback)(update);
                }
                Err(e) => {
                    let update = StreamingUpdate::Error(e.to_string());
                    (self.callback)(update);
                    return Err(e);
                }
            }
        }
        
        Ok(accumulator)
    }
}