# OAuth Login Implementation Analysis

## Overview
This document contains comprehensive findings from analyzing the OAuth login implementation in the JavaScript tool (test-fixed.js) and identifying issues with the Rust implementation.

## The Problem
When users with Max accounts attempt to login via the `/login` command, they encounter an "Invalid request format" error after approving the OAuth request. The browser makes a POST request to `https://claude.ai/v1/oauth/{organization_uuid}/authorize` which fails with a 400 error.

## Request/Response Analysis

### Failed Request Details
**URL:** `POST https://claude.ai/v1/oauth/1ec57bfa-72d9-434f-bff2-eff5376f9b74/authorize`

**Key Request Headers:**
- `Content-Type: application/json`
- `Content-Length: 348` (indicating a JSON payload is sent)
- `Cookie: sessionKey=sk-ant-sid01-...` (user is already authenticated)
- `Referer: https://claude.ai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e...`

**Response:**
```json
{
  "type": "error",
  "error": {
    "type": "invalid_request_error",
    "message": "Invalid request format"
  },
  "request_id": "req_011CT3buerxfNCoxi56cxaZX"
}
```

## JavaScript Implementation Analysis

### 1. OAuth Configuration (obj16 - Production)
```javascript
CONSOLE_AUTHORIZE_URL: "https://console.anthropic.com/oauth/authorize"
CLAUDE_AI_AUTHORIZE_URL: "https://claude.ai/oauth/authorize"
TOKEN_URL: "https://console.anthropic.com/v1/oauth/token"
CLIENT_ID: "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
SCOPES: ["org:create_api_key", "user:profile", "user:inference"]
REDIRECT_PORT: 54545
```

### 2. OAuth URL Construction (stringDecoder90)
The JavaScript builds the authorization URL with these parameters in exact order:
1. `code=true`
2. `client_id` 
3. `response_type=code`
4. `redirect_uri` (conditional based on manual vs automatic)
5. `scope` (space-separated scopes)
6. `code_challenge` (PKCE)
7. `code_challenge_method=S256`
8. `state`

### 3. OAuth Endpoint Selection Logic
```javascript
function stringDecoder90({ loginWithClaudeAi }) {
  let config8205 = loginWithClaudeAi
    ? BB().CLAUDE_AI_AUTHORIZE_URL      // "https://claude.ai/oauth/authorize"
    : BB().CONSOLE_AUTHORIZE_URL;       // "https://console.anthropic.com/oauth/authorize"
  // ... rest of OAuth URL construction
}
```

### 4. Login Command Flow
1. User types `/login`
2. System checks if already logged in via `checker265()`
3. If not logged in, calls `stringDecoder315()` to start auth flow
4. User is presented with choice:
   - "Claude account with subscription" → uses claude.ai OAuth
   - "Anthropic Console account" → uses console.anthropic.com OAuth
5. OAuth flow begins with selected endpoint

### 5. Account Type Detection
```javascript
var UZ = value1438(() => {
  let next2170 = XJ().read()?.claudeAiOauth;
  if (!next2170?.accessToken) return null;
  if (!next2170.subscriptionType) {
    let input20347 = next2170.isMax === false ? "pro" : "max";
    return { ...next2170, subscriptionType: input20347 };
  }
  return next2170;
});
```

### 6. Authentication Source Detection
```javascript
function checker51() {
  if (process.env.ANTHROPIC_AUTH_TOKEN) return { source: "ANTHROPIC_AUTH_TOKEN" };
  if (MS()) return { source: "apiKeyHelper" };
  let config8199 = UZ(); // Gets claudeAiOauth from storage
  if (rM(config8199?.scopes) && config8199?.accessToken) return { source: "claude.ai" };
  return { source: "none" };
}
```

## Critical Findings

### 1. The Organization-Specific Authorize Endpoint
- When using claude.ai OAuth, the browser makes a POST to `/v1/oauth/{org-uuid}/authorize`
- This endpoint is NOT found in the JavaScript code's OAuth URL construction
- It appears to be triggered by claude.ai's frontend after the user approves
- The endpoint expects a specific JSON payload that we're not providing

### 2. Max Account vs Pro Account OAuth
- **Max accounts** should use `console.anthropic.com` OAuth
- **Pro accounts** can use `claude.ai` OAuth
- The JavaScript doesn't automatically detect this - it asks users to choose
- Our Rust implementation defaults to console.anthropic.com (correct for Max)

### 3. The Real Issue
The problem occurs because:
1. Max account users are being redirected through claude.ai
2. Claude.ai recognizes they have a Max account and shows "Use Max account" button
3. When they click it and approve, claude.ai tries to POST to `/v1/oauth/{org-uuid}/authorize`
4. This POST fails because the OAuth flow wasn't properly initialized for organization-scoped access

### 4. Callback Server Implementation Differences
**JavaScript (MCP OAuth - more sophisticated):**
- Checks for `error`, `error_description`, `error_uri` parameters
- Validates state parameter exactly
- Returns proper HTML error pages

**JavaScript (Regular OAuth - simpler):**
- Only checks for `code` and `state` parameters
- Basic validation and error handling

## Solution

### Current Rust Implementation Status
Our Rust implementation:
- Uses `console.anthropic.com` by default (correct)
- Has `use_claude_ai` flag set to `false` (correct)
- Callback server now handles error parameters
- Logs all received parameters for debugging

### Required Fix
The OAuth flow should ALWAYS use `console.anthropic.com` for ALL users because:
1. It works for both Max and Pro accounts
2. It avoids the organization-specific authorize endpoint issue
3. It handles login if user isn't already authenticated

### Implementation Note
The JavaScript tool's approach of asking users to choose between endpoints is flawed for Max accounts. Our approach of always using console.anthropic.com is actually better and should work for all account types.

## Token Exchange Details

### Request Format (MvA function)
The token exchange uses JSON format with these parameters:
```javascript
{
  grant_type: "authorization_code",
  code: input20325,
  redirect_uri: /* conditional */,
  client_id: BB().CLIENT_ID,
  code_verifier: next2170,
  state: config8199  // Note: state is included in token exchange
}
```

**Headers:**
```javascript
{
  "Content-Type": "application/json"  // Note: Uses JSON, not form-encoded
}
```

## Conclusion
The issue stems from the OAuth flow being initiated with parameters that don't properly handle the organization-scoped authorization that claude.ai attempts when it detects a Max account. The solution is to ensure we ALWAYS use console.anthropic.com which handles all account types correctly without requiring the problematic organization-specific authorize endpoint.