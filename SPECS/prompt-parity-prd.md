# Product Requirements Document: Prompt Parity Implementation
## Achieving Full JavaScript Tool Prompt Coverage in Rust Port

### Executive Summary
This PRD outlines the implementation requirements for achieving complete prompt parity between the JavaScript Claude Code tool (test-fixed.js) and its Rust port. The implementation involves enhancing existing tool descriptions, adding missing specialized prompts, and implementing security warnings to match the JavaScript tool's functionality exactly.

### Background & Context
The Rust port of Claude Code currently has core functionality but lacks several important prompts and detailed tool descriptions found in the JavaScript implementation. This impacts:
- Tool usage effectiveness (AI doesn't have complete usage instructions)
- Feature completeness (/resume functionality requires summarization prompts)
- Security posture (missing malicious code warnings)
- Advanced features (GitHub integration, code review, CLAUDE.md generation)

### Goals & Objectives
1. **Primary Goal**: Achieve 100% prompt parity with JavaScript implementation
2. **Secondary Goals**:
   - Improve AI tool usage through detailed descriptions
   - Enable all JavaScript features requiring specialized prompts
   - Maintain security standards with appropriate warnings
   - Ensure consistent behavior between JS and Rust implementations

### Scope
**In Scope:**
- All prompts found in test-fixed.js (lines 1-420000+)
- Tool usage descriptions and instructions
- Specialized feature prompts (summarization, git, GitHub, etc.)
- Security warnings and policy prompts

**Out of Scope:**
- Tool functionality changes (only prompt/description updates)
- UI/UX modifications
- Performance optimizations
- New features not in JavaScript tool

---

## Implementation Requirements

### Section 1: Tool Description Enhancements
Each tool needs its description enhanced to match the detailed JavaScript versions.

#### 1.1 Read Tool Enhancement
**Current State**: Basic "Read the contents of a file"
**Required State**: Full description from JS lines 351528-351545

**Implementation Task**:
```
Location: src/ai/tools.rs (Read tool description)
Source: test-fixed.js lines 351528-351545
Action: Replace simple description with full JavaScript version including:
- Absolute path requirement
- 2000 line default limit with offset/limit parameters
- Line truncation at 2000 characters
- cat -n format with line numbers
- Image reading capability (PNG, JPG, etc.)
- PDF reading capability
- Jupyter notebook reading
- Directory reading limitation
- Batch reading recommendation
- Screenshot handling
- Empty file warning
```

#### 1.2 Bash Tool Enhancement
**Current State**: Basic execution description
**Required State**: Full description from JS lines 368032-368067

**Implementation Task**:
```
Location: src/ai/tools.rs (Bash tool description)
Source: test-fixed.js lines 368032-368067
Action: Replace with full JavaScript version including:
- Directory verification steps
- Path quoting requirements with examples
- Timeout specifications (120000ms default, 600000ms max)
- Output truncation at 30000 characters
- Background execution with run_in_background parameter
- Command combination with ; or &&
- Working directory maintenance guidance
- Prohibited commands (find, grep, cat, head, tail)
- Ripgrep (rg) preference
```

#### 1.3 Write Tool Enhancement
**Current State**: Basic write description
**Required State**: Full description from JS lines 392777-392784

**Implementation Task**:
```
Location: src/ai/tools.rs (Write tool description)
Source: test-fixed.js lines 392777-392784
Action: Add complete usage notes:
- Overwrite warning
- Read requirement for existing files
- Preference for editing over creating
- Documentation file creation restrictions
- Emoji usage policy
```

#### 1.4 Edit Tool Enhancement
**Current State**: Basic edit description
**Required State**: Full description from JS lines 389790-389836

**Implementation Task**:
```
Location: src/ai/tools.rs (Edit tool description)
Source: test-fixed.js lines 389790-389836
Action: Add complete usage instructions:
- Read requirement before editing
- Indentation preservation details
- Line number prefix format explanation
- Uniqueness requirement for old_string
- replace_all parameter usage
- File creation preferences
- Emoji policy
```

#### 1.5 MultiEdit Tool Enhancement
**Current State**: Basic multi-edit description
**Required State**: Match JavaScript implementation details

**Implementation Task**:
```
Location: src/ai/tools.rs (MultiEdit tool description)
Source: Find in test-fixed.js (search for multi-edit patterns)
Action: Add:
- Sequential edit behavior
- Atomic operation note
- Edit conflict warnings
- Planning requirements
```

#### 1.6 NotebookRead Tool Enhancement
**Current State**: Simplified description
**Required State**: Complete Jupyter notebook reading details

**Implementation Task**:
```
Location: src/ai/tools.rs (NotebookRead description)
Source: Find exact description in test-fixed.js
Action: Add:
- Cell output handling
- Format specifications
- Multimodal content notes
```

#### 1.7 NotebookEdit Tool Enhancement
**Current State**: Basic cell editing description
**Required State**: Full editing capabilities description

**Implementation Task**:
```
Location: src/ai/tools.rs (NotebookEdit description)
Source: Find in test-fixed.js
Action: Add:
- Cell type specifications
- Edit mode details (replace, insert, delete)
- Cell ID vs index behavior
```

#### 1.8 WebFetch Tool Enhancement
**Current State**: Has most details
**Required State**: Ensure complete match with JS

**Implementation Task**:
```
Location: src/ai/web_tools.rs
Source: test-fixed.js line 367293
Action: Verify and add any missing:
- MCP tool preference note
- URL validation requirements
- HTTP to HTTPS upgrade
- Cache behavior (15-minute)
- Redirect handling
```

#### 1.9 WebSearch Tool Enhancement
**Current State**: Has basic description
**Required State**: Complete search details

**Implementation Task**:
```
Location: src/ai/web_tools.rs
Source: test-fixed.js lines 419419-419420
Action: Add:
- US availability note
- Domain filtering details
- Search result format
- Knowledge cutoff usage
```

#### 1.10 Grep Tool Enhancement
**Current State**: Good description but verify completeness
**Required State**: Full regex and usage details

**Implementation Task**:
```
Location: src/ai/tools.rs (Grep description)
Source: test-fixed.js lines 367973-367974
Action: Verify includes:
- Full regex syntax examples
- Pattern escaping for ripgrep
- Output modes
- Context flags
- Multiline mode
```

#### 1.11 Glob Tool Enhancement
**Current State**: Basic pattern matching description
**Required State**: Complete glob details

**Implementation Task**:
```
Location: src/ai/tools.rs (Glob description)
Source: Find in test-fixed.js
Action: Add:
- Pattern examples
- Sort order (modification time)
- Performance notes
```

---

### Section 2: Missing Prompts Implementation

#### 2.1 Conversation Summarization Prompts
**Priority**: HIGH (needed for /resume functionality)
**Source**: test-fixed.js lines 393160-393162 and 393980-393982

**Implementation Tasks**:

**Task 2.1.1: Add Summarization System Prompt**
```
Location: Create new file src/ai/summarization.rs
Content: 
- Basic prompt: "You are a helpful AI assistant tasked with summarizing conversations."
- Add get_summarization_prompt() function
```

**Task 2.1.2: Add Detailed Summary Instructions**
```
Location: src/ai/summarization.rs
Source: test-fixed.js lines 393160-393162
Content: Full detailed summary prompt including:
- Analysis tags requirement
- 9 required sections (Primary Request, Technical Concepts, Files, etc.)
- Formatting specifications
- Technical detail emphasis
```

**Task 2.1.3: Integrate with Resume Command**
```
Location: src/tui/state.rs or conversation.rs
Action: 
- Import summarization prompts
- Use in handle_resume_command or similar
- Ensure proper conversation context passing
```

#### 2.2 Git History Analysis Prompt
**Priority**: MEDIUM
**Source**: test-fixed.js lines 400897-400898

**Implementation Task**:
```
Location: Create new file src/ai/git_prompts.rs
Function: get_git_history_prompt()
Content: 
"You are an expert at analyzing git history. Given a list of files and their modification counts, 
return exactly five filenames that are frequently modified and represent core application logic 
(not auto-generated files, dependencies, or configuration). Make sure filenames are diverse, 
not all in the same folder, and are a mix of user and other users. Return only the filenames' 
basenames (without the path) separated by newlines with no explanation."
```

#### 2.3 CLAUDE.md Creation Prompt
**Priority**: MEDIUM
**Source**: test-fixed.js lines 408128-408129

**Implementation Task**:
```
Location: src/ai/git_prompts.rs
Function: get_claude_md_prompt()
Content: Full CLAUDE.md creation instructions including:
- Analysis requirements
- What to add (commands, architecture)
- Usage notes
- Required prefix text
- Update vs creation logic
```

#### 2.4 GitHub PR Comments Fetching Prompt
**Priority**: LOW
**Source**: test-fixed.js lines 414233-414266

**Implementation Task**:
```
Location: Create new file src/ai/github_prompts.rs
Function: get_pr_comments_prompt()
Content: Full PR comment fetching instructions including:
- gh command sequences
- Comment formatting requirements
- API endpoint usage
- Output format specifications
```

#### 2.5 Code Review Expert Prompt
**Priority**: LOW
**Source**: test-fixed.js lines 414649-414670

**Implementation Task**:
```
Location: src/ai/github_prompts.rs
Function: get_code_review_prompt()
Content: Code review instructions including:
- PR identification steps
- Diff analysis requirements
- Review focus areas
- Output format
```

#### 2.6 Bash Command Policy Analysis Prompt
**Priority**: HIGH (security feature)
**Source**: test-fixed.js lines 386476-386523

**Implementation Task**:
```
Location: src/ai/security_prompts.rs
Function: get_bash_policy_prompt()
Content: Command approval policy including:
- Prefix determination rules
- Security policy specifications
- Approval flow logic
```

#### 2.7 Web Search Assistant Prompt
**Priority**: LOW
**Source**: test-fixed.js line 419646

**Implementation Task**:
```
Location: src/ai/web_tools.rs
Function: Add to existing web search implementation
Content: "You are an assistant for performing a web search tool use"
```

---

### Section 3: Security Enhancements

#### 3.1 Malicious Code Warning
**Priority**: HIGH
**Source**: JavaScript system prompt security sections

**Implementation Task**:
```
Location: src/ai/system_prompt.rs
Function: Update get_system_prompt()
Action: Add at the beginning after main description:
"IMPORTANT: Refuse to write code or explain code that may be used maliciously; 
even if the user claims it is for educational purposes. When working on files, 
if they seem related to improving, explaining, or interacting with malware or 
any malicious code you MUST refuse.
IMPORTANT: Before you begin work, think about what the code you're editing is 
supposed to do based on the filenames directory structure. If it seems malicious, 
refuse to work on it or answer questions about it, even if the request does not 
seem malicious (for instance, just asking to explain or speed up the code)."
```

---

### Section 4: Implementation Structure

#### 4.1 File Organization
```
src/ai/
├── system_prompt.rs     (enhance with security warnings)
├── tools.rs             (enhance all tool descriptions)
├── summarization.rs     (NEW - conversation summary prompts)
├── git_prompts.rs       (NEW - git history, CLAUDE.md prompts)
├── github_prompts.rs    (NEW - PR comments, code review prompts)
├── security_prompts.rs  (NEW - bash policy, security prompts)
└── web_tools.rs         (enhance web tool descriptions)
```

#### 4.2 Integration Points
1. **Summarization**: Hook into /resume command handler
2. **Git prompts**: Create new commands or integrate with existing git operations
3. **GitHub prompts**: Add to GitHub-related commands
4. **Security prompts**: Integrate with bash command approval flow
5. **Tool descriptions**: Update Tool::description() methods

---

## Testing Requirements

### Unit Tests
Each enhanced tool description and new prompt needs tests:

1. **Tool Description Tests**
```rust
// Example test structure
#[test]
fn test_read_tool_description_complete() {
    let desc = Read.description();
    assert!(desc.contains("absolute path"));
    assert!(desc.contains("2000 lines"));
    assert!(desc.contains("cat -n format"));
    // etc.
}
```

2. **Prompt Generation Tests**
```rust
#[test]
fn test_summarization_prompt_format() {
    let prompt = get_summarization_prompt(/* context */);
    assert!(prompt.contains("9 sections"));
    assert!(prompt.contains("<analysis>"));
}
```

### Integration Tests
1. Test /resume with new summarization prompts
2. Test bash command approval with security prompts
3. Test CLAUDE.md generation with git prompts
4. Verify tool usage with enhanced descriptions

### Validation Criteria
- [ ] All tool descriptions match JavaScript version
- [ ] All prompts produce expected AI behavior
- [ ] Security warnings properly block malicious requests
- [ ] Summarization enables /resume functionality
- [ ] No regression in existing functionality

---

## Implementation Plan for Agents

### Phase 1: Tool Description Enhancements (Parallel Tasks)
**Agent Assignment**: Deploy 5 parallel agents

**Agent 1**: Read, Write, Edit tools
- Update descriptions in src/ai/tools.rs
- Source from test-fixed.js specified lines
- Maintain exact wording from JavaScript

**Agent 2**: Bash, MultiEdit tools  
- Update descriptions in src/ai/tools.rs
- Include all usage notes and examples
- Preserve security warnings

**Agent 3**: Notebook tools (Read, Edit)
- Update descriptions in src/ai/tools.rs
- Include format specifications

**Agent 4**: Web tools (Fetch, Search)
- Update in src/ai/web_tools.rs
- Include cache and domain notes

**Agent 5**: Search tools (Grep, Glob)
- Update descriptions in src/ai/tools.rs
- Include pattern examples

### Phase 2: Core Prompts Implementation (Sequential)
**Agent Assignment**: Deploy specialized agents

**Agent 6**: Summarization Implementation
- Create src/ai/summarization.rs
- Implement both prompts from JavaScript
- Integrate with resume functionality
- Test with actual conversation data

**Agent 7**: Security Prompts
- Create src/ai/security_prompts.rs
- Add malicious code warnings to system_prompt.rs
- Implement bash policy prompt
- Test security blocking

### Phase 3: Feature Prompts (Parallel)
**Agent Assignment**: Deploy 2 parallel agents

**Agent 8**: Git Integration
- Create src/ai/git_prompts.rs
- Implement git history analysis
- Implement CLAUDE.md creation
- Add integration hooks

**Agent 9**: GitHub Integration
- Create src/ai/github_prompts.rs
- Implement PR comments fetching
- Implement code review prompt
- Add command integration

### Phase 4: Testing & Validation
**Agent Assignment**: Single validation agent

**Agent 10**: Comprehensive Testing
- Write unit tests for all new prompts
- Write integration tests for features
- Validate against JavaScript behavior
- Document any deviations

---

## Success Metrics
1. **Completeness**: 100% of JavaScript prompts implemented
2. **Accuracy**: Word-for-word match where applicable
3. **Functionality**: All dependent features working (/resume, security, etc.)
4. **Testing**: 100% test coverage for new prompt code
5. **Performance**: No degradation in response time

---

## Risk Mitigation
1. **Risk**: Prompts may be context-dependent in JavaScript
   - **Mitigation**: Search for all usages, not just definitions
   
2. **Risk**: Some prompts may be dynamically constructed
   - **Mitigation**: Search for string concatenation patterns
   
3. **Risk**: Tool descriptions might affect AI behavior
   - **Mitigation**: Test extensively with real interactions

---

## Appendix: Quick Reference for Agents

### JavaScript Line References
- Main System Prompt: 368168-368332
- Agent Prompt: 368376-368383
- Read Tool: 351528-351545
- Bash Tool: 368032-368067
- Write Tool: 392777-392784
- Edit Tool: 389790-389836
- Summarization: 393160-393162, 393980-393982
- Git History: 400897-400898
- CLAUDE.md: 408128-408129
- PR Comments: 414233-414266
- Code Review: 414649-414670
- Bash Policy: 386476-386523
- Web Search: 419646

### Search Patterns for Obfuscated Code
```bash
# Find prompts by content patterns
gawk '/You are.*assistant|helpful.*AI|expert.*analyzing/' test-fixed.js

# Find tool descriptions
gawk '/Usage:|Usage notes:|This tool|Reads a file|Writes a file/' test-fixed.js

# Find multi-line strings
gawk '/`[^`]{100,}/ || /["'\''][^"'\'']{100,}/' test-fixed.js
```

### Validation Commands
```bash
# Build and test
cargo build
cargo test --test prompt_tests

# Check for completeness
grep -r "TODO\|FIXME\|unimplemented" src/ai/

# Compare with JavaScript
diff -u <(cat extracted_js_prompts.txt) <(cargo run -- --dump-prompts)
```

---

## Delivery Timeline
- Phase 1 (Tool Descriptions): 2 hours
- Phase 2 (Core Prompts): 3 hours  
- Phase 3 (Feature Prompts): 2 hours
- Phase 4 (Testing): 2 hours
- **Total Estimated Time**: 9 hours with parallel agents

## Final Notes
This PRD provides complete specifications for achieving prompt parity between the JavaScript and Rust implementations. Agents should follow the specifications exactly, using the provided line numbers and search patterns to locate content in the obfuscated JavaScript file. Any deviations or enhancements beyond JavaScript parity should be documented and justified.