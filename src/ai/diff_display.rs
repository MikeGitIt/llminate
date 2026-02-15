use similar::{ChangeTag, TextDiff};
use std::fmt::Write;

/// Generate a unified diff display for file edits
pub struct DiffDisplay {
    old_content: String,
    new_content: String,
    file_path: String,
}

impl DiffDisplay {
    pub fn new(old_content: String, new_content: String, file_path: String) -> Self {
        Self {
            old_content,
            new_content,
            file_path,
        }
    }
    
    /// Generate a summary message with additions and removals count
    pub fn summary(&self) -> String {
        let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
        
        let mut additions = 0;
        let mut removals = 0;
        
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => additions += 1,
                ChangeTag::Delete => removals += 1,
                ChangeTag::Equal => {}
            }
        }
        
        format!(
            "Updated {} with {} addition{} and {} removal{}",
            self.file_path,
            additions,
            if additions == 1 { "" } else { "s" },
            removals,
            if removals == 1 { "" } else { "s" }
        )
    }
    
    /// Generate a colored diff display for terminal output
    pub fn colored_diff(&self, max_lines: Option<usize>) -> String {
        let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
        let mut output = String::new();
        let mut line_count = 0;
        
        // Add file header
        writeln!(&mut output, "--- {}", self.file_path).unwrap();
        writeln!(&mut output, "+++ {}", self.file_path).unwrap();
        
        // Group changes into hunks for better readability
        let changes: Vec<_> = diff.iter_all_changes().collect();
        if changes.is_empty() {
            return output;
        }
        
        let mut hunk_start = 0;
        let mut in_hunk = false;
        let mut hunk_old_line = 1;
        let mut hunk_new_line = 1;
        let mut hunk_lines = Vec::new();
        
        for (idx, change) in changes.iter().enumerate() {
            let is_change = matches!(change.tag(), ChangeTag::Insert | ChangeTag::Delete);
            
            if is_change && !in_hunk {
                // Start a new hunk
                in_hunk = true;
                hunk_start = idx.saturating_sub(3); // Include 3 lines of context before
                hunk_old_line = hunk_start + 1;
                hunk_new_line = hunk_start + 1;
                hunk_lines.clear();
                
                // Add context lines before the change
                for i in hunk_start..idx {
                    if let Some(ctx_change) = changes.get(i) {
                        hunk_lines.push(format!(" {}", ctx_change.value()));
                    }
                }
            }
            
            if in_hunk {
                // Add the current line to the hunk
                match change.tag() {
                    ChangeTag::Delete => {
                        hunk_lines.push(format!("-{}", change.value()));
                        hunk_old_line += 1;
                    }
                    ChangeTag::Insert => {
                        hunk_lines.push(format!("+{}", change.value()));
                        hunk_new_line += 1;
                    }
                    ChangeTag::Equal => {
                        hunk_lines.push(format!(" {}", change.value()));
                        hunk_old_line += 1;
                        hunk_new_line += 1;
                        
                        // Check if we should end the hunk (3 lines of context after changes)
                        let mut context_count = 0;
                        for j in (idx + 1)..changes.len() {
                            if let Some(next_change) = changes.get(j) {
                                if matches!(next_change.tag(), ChangeTag::Insert | ChangeTag::Delete) {
                                    break;
                                }
                                context_count += 1;
                                if context_count >= 3 {
                                    // End the hunk after 3 lines of context
                                    for k in (idx + 1)..=(idx + 3).min(changes.len() - 1) {
                                        if let Some(ctx_change) = changes.get(k) {
                                            if matches!(ctx_change.tag(), ChangeTag::Equal) {
                                                hunk_lines.push(format!(" {}", ctx_change.value()));
                                            }
                                        }
                                    }
                                    
                                    // Output the hunk
                                    if let Some(max) = max_lines {
                                        if line_count + hunk_lines.len() > max {
                                            writeln!(&mut output, "... ({} more lines) ...", 
                                                hunk_lines.len() - (max - line_count)).unwrap();
                                            break;
                                        }
                                    }
                                    
                                    writeln!(&mut output, "@@ -{},{} +{},{} @@",
                                        hunk_start + 1, hunk_lines.len(),
                                        hunk_start + 1, hunk_lines.len()).unwrap();
                                    
                                    for line in &hunk_lines {
                                        writeln!(&mut output, "{}", line).unwrap();
                                        line_count += 1;
                                    }
                                    
                                    in_hunk = false;
                                    hunk_lines.clear();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Output any remaining hunk
        if in_hunk && !hunk_lines.is_empty() {
            if let Some(max) = max_lines {
                if line_count + hunk_lines.len() > max {
                    writeln!(&mut output, "... ({} more lines) ...", 
                        hunk_lines.len() - (max - line_count)).unwrap();
                    return output;
                }
            }
            
            writeln!(&mut output, "@@ -{},{} +{},{} @@",
                hunk_start + 1, hunk_lines.len(),
                hunk_start + 1, hunk_lines.len()).unwrap();
            
            for line in &hunk_lines {
                writeln!(&mut output, "{}", line).unwrap();
            }
        }
        
        output
    }
    
    /// Generate a simple inline diff for small changes
    pub fn inline_diff(&self) -> String {
        let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
        let mut output = String::new();
        
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => {
                    write!(&mut output, "- {}", change.value()).unwrap();
                }
                ChangeTag::Insert => {
                    write!(&mut output, "+ {}", change.value()).unwrap();
                }
                ChangeTag::Equal => {
                    // Skip unchanged lines in inline diff
                }
            }
        }
        
        output
    }
}