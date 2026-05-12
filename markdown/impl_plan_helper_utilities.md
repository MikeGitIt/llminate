# Implementation Plan: Helper Utilities
Generated: 2025-09-12
Status: PLANNING ONLY - NO CODE IMPLEMENTED

## Functions to Implement
1. isIdentityExpired()
2. memoizeIdentityProvider()
3. httpSigningMiddleware()
4. getUserAgentMiddleware() (vlA)

## Implementation Order
1. isIdentityExpired() - Core utility for token management
2. memoizeIdentityProvider() - Caching foundation
3. getUserAgentMiddleware() - Simple middleware
4. httpSigningMiddleware() - Complex middleware using above

## Dependencies
### External Crates Required
- [ ] chrono - for timestamp handling
- [ ] cached - for memoization support
- [ ] tower - for middleware patterns
- [ ] http - for header manipulation

### Internal Dependencies
- [ ] Identity types from auth module
- [ ] Request/Response types
- [ ] Middleware traits

## For Each Function:

### Function: isIdentityExpired()
**JavaScript Location**: Line [SEARCH REQUIRED]
**Purpose**: Checks if identity needs refresh

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

### Function: memoizeIdentityProvider()
**JavaScript Location**: Line [SEARCH REQUIRED]
**Purpose**: Caches identity providers

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

### Function: httpSigningMiddleware()
**JavaScript Location**: Line [SEARCH REQUIRED]
**Purpose**: Middleware for request signing

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

### Function: getUserAgentMiddleware() (vlA)
**JavaScript Location**: Line [SEARCH REQUIRED]
**Purpose**: Adds user agent headers

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
  - Middleware pattern differences between JS and Rust
  - Caching/memoization thread safety
  - Async middleware composition
- Unknown factors:
  - Exact middleware chain order
  - Cache invalidation strategy
  - Identity expiration buffer time
- Need to research:
  - Tower middleware patterns
  - Rust memoization libraries
  - Async trait patterns

## Estimated Effort
- Simple functions (< 1 hour): isIdentityExpired(), getUserAgentMiddleware()
- Medium functions (1-3 hours): memoizeIdentityProvider()
- Complex functions (3+ hours): httpSigningMiddleware()

## Notes
- Middleware pattern in Rust differs significantly from JavaScript
- May need to use tower::Service trait
- Caching must be thread-safe (Arc<RwLock<>> or similar)
- Consider using lazy_static for global cache