# Implementation Plan: Authentication Orchestration
Generated: 2025-09-14
Status: COMPLETE EXTRACTION AVAILABLE

## Overview
This plan covers the implementation of authentication orchestration functions that coordinate and manage the overall authentication flow, including method selection, validation, error handling, and header construction.

## Extracted Functions Available
Complete implementations extracted to: `auth_orchestration_extracted.js`

## Core Orchestration Components

### 1. Authentication Method Selection

#### setupAuthentication
**JavaScript Location**: Main orchestration function in extraction
**Purpose**: Coordinates auth method selection and setup
**Key Features**:
- Checks available authentication methods
- Prioritizes: OAuth token → API key → OAuth flow
- Validates selected method
- Constructs appropriate headers
- Handles fallback scenarios

#### validateHeaders
**Purpose**: Validates which auth methods are available
**Key Features**:
- Checks for API key presence
- Validates OAuth token availability
- Determines auth capability

#### shouldUseOAuthFlow
**Purpose**: Determines when OAuth should be used
**Key Features**:
- Environment detection
- Token availability checking
- Fallback logic

### 2. API Key Management

#### resolveApiKey
**Purpose**: Resolves API key from multiple sources
**Key Features**:
- Environment variables (ANTHROPIC_API_KEY)
- Stored credentials
- Approved key lists
- Helper scripts

#### isApiKeyApproved
**Purpose**: Validates API keys against approved lists
**Key Features**:
- Whitelist checking
- Format validation
- Security compliance

#### maskApiKey
**Purpose**: Safely masks API keys for display
**Key Features**:
- Shows first/last few characters
- Security for logging
- User-friendly display

### 3. OAuth Token Management

#### refreshAccessToken
**Purpose**: Handles OAuth token refresh
**Key Features**:
- Token exchange logic
- Refresh token usage
- Expiration handling
- Error recovery

#### checkAuthTokenStatus
**Purpose**: Validates OAuth token status
**Key Features**:
- Token existence check
- Expiration validation
- Scope verification

### 4. Header Construction

#### authHeaders
**Purpose**: Builds X-Api-Key headers
**Key Features**:
- API key formatting
- Header name configuration
- Value validation

#### authTokenHeaders
**Purpose**: Builds Authorization Bearer headers
**Key Features**:
- Bearer token formatting
- OAuth token inclusion
- Standard compliance

#### setupBearerAuth
**Purpose**: Configures Bearer authentication
**Key Features**:
- Bearer prefix addition
- Token validation
- Header construction

#### parseCustomHeaders
**Purpose**: Parses custom headers from environment
**Key Features**:
- Environment variable parsing
- Header name/value extraction
- Validation and sanitization

### 5. HTTP Authentication Schemes

#### defaultHttpAuthSchemeProvider
**Purpose**: Provides available auth schemes
**Key Features**:
- Scheme enumeration
- Priority ordering
- Capability detection

#### resolveHttpAuthSchemeConfig
**Purpose**: Configures HTTP authentication
**Key Features**:
- Scheme selection
- Parameter configuration
- Signer setup

#### HttpBearerAuthSigner
**Purpose**: Signs requests with Bearer tokens
**Key Features**:
- Request signing
- Token injection
- Header management

### 6. Error Handling & Retry

#### handleAuthError
**Purpose**: User-friendly auth error handling
**Key Features**:
- Error message formatting
- Status code interpretation
- Recovery suggestions

#### setupAuthRetryConfig
**Purpose**: Configures auth retry logic
**Key Features**:
- Exponential backoff
- Max retry attempts
- Retry conditions

#### refreshRetryTokenForRetry
**Purpose**: Handles retry token refresh
**Key Features**:
- Token refresh on retry
- Backoff calculation
- Success/failure tracking

### 7. Environment Management

#### validateEnvironmentId
**Purpose**: Validates auth environment setup
**Key Features**:
- Environment variable checking
- Required variable validation
- Configuration verification

## Implementation Order

### Phase 1: Core Infrastructure
1. Error types and constants
2. Environment variable handling
3. Basic validation functions

### Phase 2: API Key Management
1. resolveApiKey - Main resolution logic
2. isApiKeyApproved - Validation
3. maskApiKey - Security utilities
4. API key header construction

### Phase 3: OAuth Management
1. OAuth token validation
2. refreshAccessToken - Token refresh
3. Bearer token setup
4. OAuth header construction

### Phase 4: Method Selection
1. validateHeaders - Capability detection
2. shouldUseOAuthFlow - OAuth decision
3. setupAuthentication - Main orchestration

### Phase 5: Advanced Features
1. Custom header parsing
2. HTTP auth schemes
3. Request signing

### Phase 6: Error & Retry
1. Error handling utilities
2. Retry configuration
3. Backoff strategies

## Rust Implementation Structure

```rust
pub mod orchestration {
    pub mod selection;     // Method selection logic
    pub mod api_key;      // API key management
    pub mod oauth;        // OAuth token management
    pub mod headers;      // Header construction
    pub mod schemes;      // HTTP auth schemes
    pub mod errors;       // Error handling
    pub mod retry;        // Retry logic
    pub mod environment;  // Environment validation
}
```

## Key Design Patterns

### 1. Strategy Pattern for Auth Methods
```rust
trait AuthStrategy {
    async fn authenticate(&self, config: &Config) -> Result<Headers>;
    fn priority(&self) -> u32;
}
```

### 2. Chain of Responsibility for Resolution
```rust
trait AuthResolver {
    async fn resolve(&self) -> Option<Credentials>;
    fn next(&self) -> Option<Box<dyn AuthResolver>>;
}
```

### 3. Builder Pattern for Configuration
```rust
pub struct AuthConfigBuilder {
    // Flexible configuration building
}
```

## Configuration Management

### Environment Variables
- ANTHROPIC_API_KEY
- ANTHROPIC_AUTH_TOKEN
- ANTHROPIC_CUSTOM_HEADERS
- ANTHROPIC_BASE_URL
- ANTHROPIC_RETRY_CONFIG

### Priority Order
1. Explicit configuration
2. Environment variables
3. Configuration files
4. Default values

## Error Handling Strategy

### Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("No authentication method available")]
    NoAuthMethod,

    #[error("Invalid API key: {0}")]
    InvalidApiKey(String),

    #[error("OAuth token expired")]
    TokenExpired,

    #[error("Authentication failed: {0}")]
    AuthFailed(String),
}
```

### Recovery Strategies
1. Automatic token refresh
2. Fallback to alternative methods
3. User prompt for credentials
4. Graceful degradation

## Testing Requirements

### Unit Tests
- Each orchestration function
- Method selection logic
- Header construction
- Error scenarios

### Integration Tests
- Full authentication flow
- Method fallback
- Token refresh cycle
- Error recovery

### Security Tests
- API key masking
- Secure storage
- Header sanitization
- Token expiration

## Security Considerations

1. **Never log full API keys**
2. **Mask sensitive data in errors**
3. **Validate all input headers**
4. **Secure token storage**
5. **Time-constant comparisons**

## Performance Considerations

1. **Cache validated credentials**
2. **Minimize validation calls**
3. **Parallel method checking**
4. **Efficient header construction**
5. **Smart retry backoff**

## Migration Notes

### From JavaScript to Rust
- Promise chains → async/await
- Dynamic typing → Strong types
- Prototype methods → Trait implementations
- Callbacks → Futures

### Obfuscation Mapping
- JavaScript uses obfuscated names
- Map to clear, descriptive Rust names
- Preserve exact logic and behavior
- Document mapping for reference