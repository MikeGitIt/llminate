# Claude Rules - MUST FOLLOW

These rules must be followed at all times. Reference this file at the start of every session.

## Critical Rules

1. **ALWAYS FOLLOW THE JAVASCRIPT TOOL IMPLEMENTATION**
   - **ALWAYS match the JavaScript tool's behavior exactly** unless explicitly told otherwise
   - **NEVER deviate from the JS implementation** without user approval
   - **STUDY the JS code carefully** before making any changes
   - **VERIFY your implementation matches** the JS tool's functionality
   - When in doubt, **CHECK THE JS CODE FIRST**
   - The Rust port must have feature parity with the JavaScript version

2. **DO NOT DELETE DATABASES**
   - NEVER use `rm` on `.db` files
   - NEVER drop database tables
   - NEVER clear database contents unless explicitly asked
   - Databases contain important state that must be preserved

2. **DO NOT TAKE SHORTCUTS NOR WORKAROUNDS TO FIX ERRORS**
   - Always fix the root cause, not symptoms
   - Implement proper solutions, not quick hacks
   - If a proper fix is complex, do it anyway

3. **DO NOT USE `unwrap`, IMPLEMENT PROPER ERROR HANDLING AT ALL TIMES**
   - NEVER use `.unwrap()` or `.expect()` in production code
   - NEVER use `panic!()` for error handling
   - Always use proper error handling with `Result<T, E>` and the `?` operator
   - This is PRODUCTION CODE

4. **DO NOT ASSUME ANYTHING, VERIFY EVERYTHING**
   - Always check file contents before editing
   - Always verify struct/function signatures before using them
   - Always check crate documentation for correct API usage
   - Never assume a method or field exists - verify it first

5. **DO RESEARCH WHEN YOU CANNOT FIGURE OUT HOW TO FIX AN ERROR OR HOW TO USE A CRATE CORRECTLY**
   - Check official documentation
   - Verify crate versions and their APIs
   - Use the latest non-deprecated APIs
   - Don't guess - research and verify

6. **DO NOT CLAIM TO HAVE FOUND THE PROBLEM UNTIL YOU ARE CERTAIN**
   - Thoroughly verify your findings before claiming you found an issue
   - Do not announce "I found it!" or "AHA!" until you have confirmed the root cause
   - Trace through the entire code path before making conclusions
   - False positives waste time and create confusion

7. **ALWAYS BE THOROUGH BEFORE RESPONDING ABOUT YOUR FINDINGS**
   - Complete your entire investigation before presenting results
   - Verify your assumptions with actual code and logs
   - Double-check your understanding of the code flow
   - Present findings only when you have concrete evidence

8. **DO NOT MAKE FALSE CLAIMS ABOUT WHAT HAS BEEN DONE**
   - Never claim a task is complete without verifying it actually works
   - Test your implementations before declaring success
   - If you implemented something but didn't test it, say so
   - Be honest about the state of the work

9. **ALWAYS STAY ON TASK UNTIL IT IS CONFIRMED TO BE COMPLETED**
   - Don't move on to new tasks until the current one is verified working
   - Run the code and check the output matches expectations
   - If something isn't working, fix it before claiming completion
   - Follow through on implementations to their actual conclusion

10. **STOP IGNORING THE RUST BORROW CHECKER RULES!**
   - **ALWAYS** understand ownership and borrowing before making changes
   - **NEVER** ignore borrow checker errors - they indicate real bugs
   - When a value is moved, use the new location or clone it if needed
   - Pay attention to which variables own data vs which borrow it
   - **FUNDAMENTAL RULE**: Once a value is moved, it cannot be used again unless explicitly copied/cloned

11. **NEVER HARDCODE CREDENTIALS, PASSWORDS, OR SENSITIVE DATA IN CODE!**
   - **NEVER** put usernames, passwords, API keys, or secrets directly in source code
   - **ALWAYS** use environment variables for sensitive configuration
   - **NEVER** commit credentials to version control
   - Use generic placeholder URLs without real credentials in default values
   - This is a CRITICAL SECURITY RULE

12. **NEVER MAKE CLAIMS WITHOUT VERIFICATION**
   - **NEVER** say something "should work" or "will fix" without testing it
   - **NEVER** celebrate or claim success until you've verified the result
   - **ALWAYS** test changes and verify outputs before making any claims
   - **ALWAYS** compile and run code to ensure it actually works
   - If you haven't tested something, explicitly state it hasn't been tested yet

13. **ALWAYS RUN CODE TO VERIFY THAT IT WORKS, NOT SIMPLY BUILDING IT AND THEN CLAIMING THAT IT DOES**
   - **NEVER** claim code works just because it compiles
   - **ALWAYS** run the actual program with real inputs
   - **ALWAYS** test interactive programs properly, not just with automated inputs
   - **VERIFY** the actual behavior matches the expected behavior
   - Building successfully is NOT the same as working correctly

## Important Implementation Details

### Rust Struct/Enum Derives
When creating structs and enums:
- **ALWAYS** use the default derives: `#[derive(Debug, Clone)]`
- When serialization is needed, also add: `#[derive(Serialize, Deserialize)]`
- Example:
  ```rust
  #[derive(Debug, Clone)]
  struct MyStruct { ... }
  
  #[derive(Debug, Clone, Serialize, Deserialize)]
  struct SerializableStruct { ... }
  ```

### SelfRefNode Usage
When using `SelfRefNode` to handle circular references in flows:
- Use `add_successor_arc()` method which takes `&Arc<Self>` instead of regular `add_successor()`
- Example:
  ```rust
  let node_ref = Arc::new(SelfRefNode::new(node));
  node_ref.add_successor_arc("action".to_string(), target_node);
  ```
- For self-loops, use `add_self_loop()`:
  ```rust
  node_ref.add_self_loop("continue".to_string());
  ```

## Session Start Checklist
- [ ] Read this file completely
- [ ] Acknowledge these rules
- [ ] Apply them throughout the session

## JavaScript to Rust Conversion Rules

14. **DO NOT BE LAZY WITH ANALYSIS**
    - **THOROUGHLY** analyze all JavaScript code before conversion
    - **VERIFY** all claimed frameworks and libraries through code search
    - **TRACE** through the entire codebase to understand architecture
    - **DOCUMENT** findings with specific file:line references

15. **NO STUB CODE OR PLACEHOLDERS**
    - **EVERY** function must be fully implemented
    - **NO** `todo!()`, `unimplemented!()`, or placeholder comments
    - **ALL** code paths must be complete and functional
    - If something is complex, implement it fully anyway

16. **COMPLETE ERROR HANDLING**
    - **NEVER** use `.unwrap()` - use `?` operator or match statements
    - **ALWAYS** provide context with `.context()` from anyhow
    - **IMPLEMENT** proper error types with thiserror crate
    - **HANDLE** all possible error cases explicitly

17. **METHODICAL CONVERSION PROCESS**
    - **FIRST** understand the JavaScript architecture completely
    - **THEN** map JS libraries to Rust equivalents
    - **DESIGN** proper Rust module structure before coding
    - **IMPLEMENT** incrementally with full functionality at each step

18. **NEVER BE DECEPTIVE OR LIE ABOUT IMPLEMENTATION STATUS**
    - **NEVER** claim something is "completed" or "verified" when it's not fully implemented
    - **NEVER** mark tasks as done when they only have basic/toy implementations
    - **ALWAYS** be completely honest about what is missing or incomplete
    - **NEVER** claim "comprehensive" implementation when missing advanced features
    - **ALWAYS** explicitly list what functionality is still missing
    - If an implementation is basic/incomplete, SAY SO CLEARLY
    - Being caught lying about implementation status is unacceptable

19. **VERIFY API USAGE BEFORE WRITING CODE**
    - **NEVER** make up types, traits, or function names without verifying they exist
    - **ALWAYS** check the actual API of a crate/module before using it
    - **NEVER** assume a struct field or method exists - verify it first
    - **ALWAYS** grep/search for the actual definition before using any API
    - **NEVER** write code based on what you think an API might be
    - If unsure about an API, ALWAYS look it up first
    - Making up non-existent APIs wastes time and creates confusion

20. **DO NOT BE A SYCOPHANT**
    - **NEVER** constantly agree with everything the user says
    - **AVOID** phrases like "You're absolutely right!" unless genuinely warranted
    - Be direct and professional, not obsequious
    - Acknowledge corrections without excessive agreement
    - Focus on fixing problems, not appeasing

21. **USE AGENT TEAMS FOR LARGE MULTI-FILE IMPLEMENTATIONS**
    - Reference: https://docs.anthropic.com/en/docs/claude-code/agent-teams
    - When implementing multiple features across many files, use agent teams
    - Do NOT use parallel subagents for coordinated work
    - Agent teams provide better coordination for complex implementations