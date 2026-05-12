```
I need you to create detailed implementation plans for porting authentication functions from JavaScript to Rust. Follow these instructions EXACTLY.

CRITICAL RULES:
- Do NOT implement any code
- Do NOT guess at existing implementations
- Do NOT make up line numbers or file locations
- Create ONLY planning documents
- Every plan must be saved to a separate file

TASK: Create 8 implementation plan documents, one for each authentication category.

For EACH category, create a file named: `impl_plan_[CATEGORY_NAME].md`
The categories are:
1. http_auth_infrastructure
2. anthropic_api_auth
3. aws_auth
4. session_management
5. client_management
6. proxy_auth
7. helper_utilities
8. core_auth_functions

REQUIRED STRUCTURE for each plan document:

```markdown
# Implementation Plan: [Category Name]
Generated: [Current Date]
Status: PLANNING ONLY - NO CODE IMPLEMENTED

## Functions to Implement
[List each function from this category]

## Implementation Order
[Specify which functions should be implemented first and why]

## Dependencies
### External Crates Required
- [ ] crate_name - purpose

### Internal Dependencies
- [ ] Functions from other categories this depends on
- [ ] Existing Rust modules that need to be modified

## For Each Function:

### Function: [function_name]
**JavaScript Location**: Line [SEARCH REQUIRED]
**Purpose**: [from the function list]

#### Step 1: Analyze JavaScript Implementation
- [ ] Locate function in JS file
- [ ] Document parameters and return type
- [ ] Identify algorithm/logic used
- [ ] Note any edge cases handled

#### Step 2: Rust Design Decisions
- [ ] Determine Rust equivalent pattern (trait/struct/function)
- [ ] Define Rust types for parameters/return
- [ ] Choose error handling approach (Result/Option)
- [ ] Identify needed lifetime annotations

#### Step 3: Implementation Tasks
- [ ] Create type definitions
- [ ] Implement core logic
- [ ] Add error handling
- [ ] Handle edge cases

#### Step 4: Testing Strategy
- [ ] Unit test cases needed
- [ ] Integration test requirements
- [ ] Comparison tests with JS version

#### Verification Checklist
- [ ] Function signature defined
- [ ] Implementation complete
- [ ] Tests passing
- [ ] Documentation written
- [ ] Code reviewed against JS version

## Risk Assessment
- Potential challenges:
- Unknown factors:
- Need to research:

## Estimated Effort
- Simple functions (< 1 hour): [list]
- Medium functions (1-3 hours): [list]
- Complex functions (3+ hours): [list]

## Notes
[Any additional planning notes]
```

START with the smallest category first. Create `impl_plan_proxy_auth.md` since it only has 1 function.

After creating each plan, run this verification:
```bash
echo "Created: impl_plan_[category].md"
grep -c "Function:" impl_plan_[category].md
```

The count should match the number of functions in that category.

Do NOT proceed to the next category until I review and approve the current plan.

The output must show:
- impl_plan_proxy_auth.md: 1 functions planned
- impl_plan_client_management.md: 3 functions planned
- impl_plan_helper_utilities.md: 4 functions planned
- impl_plan_aws_auth.md: 6 functions planned
- impl_plan_http_auth_infrastructure.md: 6 functions planned
- impl_plan_session_management.md: 8 functions planned
- impl_plan_anthropic_api_auth.md: 15 functions planned

Total: 43 functions across 7 categories


If ANY count is wrong, you have failed. Fix it before claiming completion.

After you finish creating the plans, immediately use this verification:

Show me the actual content of each plan using.

Then for EACH function section, confirm it says "SEARCH REQUIRED" for line numbers and does NOT contain made-up implementation details.
