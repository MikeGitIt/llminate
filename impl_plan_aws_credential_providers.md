# Implementation Plan: AWS Credential Providers
Generated: 2025-09-14
Status: COMPLETE EXTRACTION AVAILABLE

## Overview
This plan covers the implementation of AWS credential provider functions that were missing from the original AWS auth implementation. These functions provide various methods for obtaining AWS credentials from different sources.

## Extracted Functions Available
Complete implementations extracted to: `aws_credential_providers_extracted.js`

## Functions to Implement

### 1. Core Credential Providers

#### fromTemporaryCredentials
**JavaScript Location**: Lines 1-94 in extraction
**Purpose**: STS temporary credentials provider with MFA support
**Key Features**:
- AssumeRole with temporary credentials
- MFA device support
- Credential provider recursion detection
- Master credentials validation

#### fromWebToken
**JavaScript Location**: Lines 96-181 in extraction
**Purpose**: Web identity token credentials provider
**Key Features**:
- AssumeRoleWithWebIdentity command
- Role assumer configuration
- Policy ARNs support
- Session duration configuration

#### fromTokenFile
**JavaScript Location**: Lines 183-225 in extraction
**Purpose**: Token file provider using environment variables
**Key Features**:
- Reads AWS_WEB_IDENTITY_TOKEN_FILE
- Uses AWS_ROLE_ARN and AWS_ROLE_SESSION_NAME
- Wraps fromWebToken functionality

#### fromSSO
**JavaScript Location**: Lines 227-347 in extraction
**Purpose**: SSO authentication provider
**Key Features**:
- SSO profile validation
- Legacy and session SSO support
- Profile conflict detection
- AWS config file parsing

#### fromCognitoIdentity
**JavaScript Location**: Lines 349-437 in extraction
**Purpose**: Cognito identity credentials provider
**Key Features**:
- GetCredentialsForIdentityCommand
- Custom role ARN support
- Login resolution
- Identity ID handling

#### fromCognitoIdentityPool
**JavaScript Location**: Lines 439-527 in extraction
**Purpose**: Cognito identity pool provider
**Key Features**:
- GetId and GetOpenIdToken commands
- Cache support for identity tokens
- Account ID extraction
- Login provider configuration

### 2. Additional Providers (from second extraction)

#### fromEnv
**JavaScript Location**: Lines 789-848 in extraction
**Purpose**: Environment variable credentials provider
**Key Features**:
- Reads AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY
- Optional AWS_SESSION_TOKEN support
- AWS_CREDENTIAL_EXPIRATION parsing
- Cross-platform environment reading

#### fromHttp
**JavaScript Location**: Lines 850-936 in extraction
**Purpose**: HTTP credentials provider for containers
**Key Features**:
- ECS/Container metadata service
- Authorization token support
- Timeout configuration
- Retry logic

#### fromContainerMetadata/fromInstanceMetadata
**JavaScript Location**: Lines 938-1024 in extraction
**Purpose**: Container and EC2 metadata providers
**Key Features**:
- Memoization support
- IMDSv2 compatibility
- Credential refresh
- Error handling

#### fromNodeProviderChain
**JavaScript Location**: Lines 1026-1094 in extraction
**Purpose**: Main credential provider chain
**Key Features**:
- Tries multiple sources in order
- Environment → SSO → INI → Process → Token → Remote
- Configurable chain
- Error aggregation

### 3. Supporting Components

#### Role Assumers
- getDefaultRoleAssumer (Lines 529-618)
- getDefaultRoleAssumerWithWebIdentity (Lines 620-705)

#### Helper Functions
- resolveLogins (Lines 707-732)
- isSsoProfile/validateSsoProfile (Lines 734-782)
- getAccountIdFromAssumedRoleUser
- resolveRegion
- decorateDefaultCredentialProvider

## Implementation Order

### Phase 1: Foundation
1. Environment variable constants
2. Error types (CredentialsProviderError)
3. Base credential provider trait
4. Helper functions (resolveRegion, etc.)

### Phase 2: Basic Providers
1. fromEnv - Simplest provider
2. fromTokenFile - File-based provider
3. fromHttp - HTTP-based provider

### Phase 3: AWS Service Providers
1. fromTemporaryCredentials - STS integration
2. fromWebToken - Web identity
3. Role assumers (needed by above)

### Phase 4: Complex Providers
1. fromSSO - SSO authentication
2. fromCognitoIdentity - Cognito integration
3. fromCognitoIdentityPool - Identity pools

### Phase 5: Chain Provider
1. fromNodeProviderChain - Main chain
2. Integration with existing providers
3. Testing full chain

## Dependencies

### External Crates Required
```toml
aws-config = "1.0"
aws-sdk-sts = "1.0"
aws-sdk-cognitoidentity = "1.0"
aws-sdk-sso = "1.0"
aws-types = "1.0"
aws-credential-types = "1.0"
```

### Internal Dependencies
- Existing AWS auth module
- HTTP client configuration
- File system utilities
- Environment variable reader

## Testing Strategy

### Unit Tests
- Each provider in isolation
- Mock AWS service responses
- Error condition handling
- Credential expiration

### Integration Tests
- Full provider chain
- Real AWS service calls (with test credentials)
- Environment variable configuration
- File-based configuration

## Rust Design Decisions

### Trait Design
```rust
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn provide_credentials(&self) -> Result<Credentials>;
}
```

### Error Handling
- Use anyhow::Result for flexibility
- Custom error types for specific failures
- Context propagation for debugging

### Configuration
- Builder pattern for complex providers
- Default implementations where sensible
- Environment variable fallbacks

## Key Implementation Notes

1. **Memoization**: Many providers cache credentials to avoid repeated calls
2. **Expiration**: All providers must check credential expiration
3. **Feature Tagging**: Credentials tagged with provider source for telemetry
4. **Recursion Detection**: Prevent infinite loops in provider chains
5. **Thread Safety**: All providers must be Send + Sync

## Migration from JavaScript

### Variable Name Mapping
- JavaScript uses obfuscated names (input20325, config8199)
- Map to descriptive Rust names
- Preserve logic exactly

### Async/Await
- JavaScript Promises → Rust async/await
- Proper error propagation with ?
- No .unwrap() in production code

### AWS SDK Differences
- JavaScript SDK v3 → Rust SDK
- Command pattern → Direct method calls
- Different error types and handling