# Authentication Functions and Variables in test-fixed.js

## Overview
This document contains a comprehensive list of all authentication-related functions, variables, and parameters found in the obfuscated test-fixed.js file (270,562 lines).

## 1. HTTP Authentication Signers

### HttpBearerAuthSigner
- **Name**: `value4480` (class)
- **Location**: Lines 92126-92141
- **Method**: `sign(input20325, config8199, next2170)`
- **Parameters**: 
  - `config8199.token` - Bearer token value
- **Related Variables**:
  - `Authorization` header field
  - Bearer token prefix

### HttpApiKeyAuthSigner
- **Name**: `value4479` (class)
- **Location**: Lines 92089-92123
- **Method**: `sign(input20325, config8199, next2170)`
- **Parameters**:
  - `config8199.apiKey` - API key value
  - `next2170.name` - Header/query parameter name
  - `next2170.in` - Location (header or query)
  - `next2170.scheme` - Optional scheme prefix
- **Related Constants**:
  - `HttpApiKeyAuthLocation.QUERY`
  - `HttpApiKeyAuthLocation.HEADER`

### NoAuthSigner
- **Name**: `value4481` (class)
- **Location**: Lines 92142-92149
- **Method**: `sign(input20325, config8199, next2170)`
- **Purpose**: Pass-through (no authentication)

## 2. Identity and Credential Management

### DefaultIdentityProviderConfig
- **Name**: `value4478` (class)
- **Location**: Lines 92075-92087
- **Method**: `getIdentityProvider(schemeId)`
- **Variables**:
  - Authentication scheme mapping object

## 3. Anthropic API Authentication

### Primary API Key Functions

#### API Key Resolution
- **Function**: `QX()`
- **Location**: Lines 355399-355431
- **Parameters**: None (reads environment)
- **Variables**:
  - `ANTHROPIC_API_KEY` (env var)
  - `apiKeyHelper` (stored config)

#### Token Availability Checker
- **Function**: `checker51()`
- **Location**: Lines 355373-355396
- **Variables**:
  - `ANTHROPIC_AUTH_TOKEN` (env var)
  - OAuth token scopes

#### Simple API Key Getter
- **Function**: `func157()`
- **Location**: Lines 355395-355398
- **Returns**: API key string or null

### Anthropic Client Constructor
- **Class**: `Class32`
- **Location**: Lines 372975-372988
- **Parameters**:
  - `baseURL` - API base URL
  - `apiKey` - API key
  - `authToken` - OAuth token
- **Environment Variables**:
  - `ANTHROPIC_BASE_URL`
  - `ANTHROPIC_API_KEY`
  - `ANTHROPIC_AUTH_TOKEN`

### OAuth Bearer Token Setup
- **Function**: `func255(input20325)`
- **Location**: Lines 378627-378632
- **Sets Headers**:
  - `Authorization: Bearer ${token}`
  - `Proxy-Authorization: Bearer ${token}`

### Custom Headers Support
- **Function**: `stringDecoder218()`
- **Location**: Lines 378633-378640
- **Environment Variable**: `ANTHROPIC_CUSTOM_HEADERS`

### Authentication Headers Builder
- **Function**: `checker91()`
- **Location**: Lines 366575-366595
- **Returns**: Headers object with auth
- **Priority Order**:
  1. OAuth token
  2. API key

## 4. OAuth Profile Management

### Profile Fetching
- **Function**: `qvA(input20325)`
- **Location**: Lines 355185-355199
- **Parameter**: `input20325` - Access token
- **Endpoint**: `/api/oauth/profile`

### Token Revocation
- **Location**: Lines 379375-379401
- **Endpoint**: `options1339.revocation_endpoint`
- **Headers**: Bearer token authorization

## 5. AWS Authentication Infrastructure

### Main Auth Feature Checker
- **Function**: `klA(input20325, config8199, next2170)`
- **Location**: Lines 100508-100545
- **Variables**:
  - `selectedHttpAuthScheme?.identity`
  - `acquireInitialRetryToken`
  - Retry mode settings

### SSO Token Management
- **Function**: `transformer262(input20325)`
- **Location**: Lines 101560-101564
- **Related Function**: `getSSOTokenFilepath()`
- **Purpose**: Read and parse SSO tokens from filesystem

### Request Signing
- **Function**: `getResolvedSigningRegion()`
- **Location**: Line 100972
- **Variables**:
  - `signingRegion`
  - `signingService`

## 6. Authentication Constants and Configuration

### Authentication Locations
- **Constants**: `HttpApiKeyAuthLocation`
- **Location**: Lines 87567-87571
- **Values**:
  - `HEADER = "header"`
  - `QUERY = "query"`

### Cryptographic Algorithms
- **Object**: `GiA`
- **Location**: Lines 101112-101116
- **Supported Algorithms**:
  - `MD5 = "md5"`
  - `SHA1 = "sha1"`
  - `SHA256 = "sha256"`

### Checksum Constructors
- **Function**: `value4771`
- **Location**: Lines 101122-101131
- **Parameters**:
  - `input20325.sha256`
  - `input20325.md5`

## 7. Session Management Functions

- **Function**: `makeSession()` - Line 4776
- **Function**: `getSession()` - Lines 4785, 4800
- **Function**: `setSession()` - Lines 4792, 4802
- **Function**: `closeSession()` - Line 4801
- **Function**: `updateSession()` - Line 4787
- **Function**: `captureSession()` - Line 4810

## 8. Proxy Authentication

### Basic Auth for Proxy
- **Location**: Lines 14515-14518
- **Implementation**:
  ```javascript
  let input20289 = `${username}:${password}`;
  config8205["Proxy-Authorization"] = `Basic ${Buffer.from(input20289).toString("base64")}`;
  ```

## 9. Middleware Authentication

### HTTP Signing Middleware
- **Location**: Lines 91785-91786
- **Functions**:
  - `httpSigningMiddleware`
  - `httpSigningMiddlewareOptions`
  - `memoizeIdentityProvider`
  - `isIdentityExpired`

### User Agent Middleware
- **Name**: `vlA`
- **Location**: Line 100633
- **Configuration**:
  - Step: "build"
  - Priority: "low"
  - Tags: ["SET_USER_AGENT", "USER_AGENT"]

## 10. Error Handling

### Authentication Error Messages
- **Location**: Lines 91854-91856
- **Message**: `"HttpAuthScheme \`${schemeId}\` did not have an IdentityProvider configured."`

### API Key Error Detection
- **Location**: Lines 373601-373616
- **Handles**: X-API-Key related errors

## 11. API Endpoints and URLs

### Base Configuration
- **Location**: Lines 341002-341007
- **URLs**:
  - `BASE_API_URL: "https://api.anthropic.com"`
  - `CONSOLE_AUTHORIZE_URL: "https://console.anthropic.com/oauth/authorize"`

## 12. Security Functions

### API Key Truncation
- **Function**: `VJ()`
- **Purpose**: Truncate API keys for display (e.g., "sk-ant-...")

### User Approval Flow
- **Location**: Lines 409095-409110
- **Purpose**: Prompt user to approve environment API keys

## 13. Client Management

- **Function**: `setClient(client)` - Line 5116
- **Function**: `getClient()` - Line 5120

## 14. Environment Variable Access Patterns

Primary authentication environment variables:
- `ANTHROPIC_API_KEY`
- `ANTHROPIC_AUTH_TOKEN`
- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_CUSTOM_HEADERS`
- `https_proxy` / `http_proxy`
- AWS/GCP/Azure environment variables

## Summary

The authentication system in test-fixed.js includes:
- **Multiple auth methods**: Bearer tokens, API keys, OAuth, Basic auth
- **Provider-specific implementations**: Anthropic, AWS, proxy servers
- **Comprehensive error handling**: Validation and user approval flows
- **Security features**: Key truncation, secure storage, environment isolation
- **Middleware architecture**: Pluggable authentication signers
- **Session management**: Full lifecycle support

Total identified authentication components: ~50+ functions and variables across multiple authentication schemes.