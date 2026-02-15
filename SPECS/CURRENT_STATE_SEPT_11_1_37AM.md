# Current State of Claude Desktop Authentication Issue - Sept 11, 1:37 AM

## Problem Summary
The Rust tool cannot authenticate using Claude Desktop OAuth tokens and keeps consuming API credits instead of using the user's Max subscription. The user gets "Your credit balance is too low" errors despite having Claude Desktop Max.

## What Works
- ✅ OAuth token detection from macOS keychain (`"Claude Code-credentials"` service)
- ✅ OAuth token parsing with correct scopes `["user:inference", "user:profile"]`
- ✅ Authentication priority logic (falls back to OAuth when API key unapproved)
- ✅ Code compiles without errors

## What Fails
- ❌ **CRITICAL**: All attempts to use OAuth tokens result in API errors
- ❌ claude.ai/api/v1/messages returns 403 with 20KB HTML error page (not an API endpoint)
- ❌ OAuth to API key conversion fails: requires `org:create_api_key` scope but tokens only have `user:inference` + `user:profile`
- ❌ api.anthropic.com rejects OAuth tokens: "OAuth authentication is currently not supported"

## Authentication Flow Attempts Made

### Attempt 1: Direct OAuth Bearer Authentication
- Used `Authorization: Bearer <token>` with `api.anthropic.com/v1/messages`
- **Result**: `401: OAuth authentication is currently not supported`

### Attempt 2: OAuth to API Key Conversion
- Used `/api/oauth/claude_cli/create_api_key` endpoint found in JavaScript code
- **Result**: `403: OAuth token lacks required scopes. Token must have the following scopes: org:create_api_key`

### Attempt 3: Claude.ai Endpoints  
- Used `https://claude.ai/api/v1/messages` with Bearer auth
- **Result**: `403 Forbidden` with 20KB HTML response (webpage, not API)

## Current Code State

### Authentication Logic (WORKING)
```rust
// In should_prefer_oauth() - CORRECTLY implemented
if auth_source.source == "ANTHROPIC_API_KEY" {
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if self.is_api_key_approved(&api_key).await? {
            debug!("Found approved ANTHROPIC_API_KEY - no OAuth");
            return Ok(false);
        } else {
            debug!("Found unapproved ANTHROPIC_API_KEY - OAuth preferred");
        }
    }
}
```

### Current Endpoint Configuration (WRONG)
```rust
match auth_method {
    AuthMethod::ApiKey(api_key) => {
        config.base_url = "https://api.anthropic.com/v1".to_string();
        // Uses x-api-key header
    }
    AuthMethod::ClaudeAiOauth(oauth_auth) => {
        config.base_url = "https://claude.ai/api/v1".to_string(); // THIS FAILS
        // Uses Authorization: Bearer header
    }
}
```

## Log Evidence

### Authentication Working Correctly
```
Found unapproved ANTHROPIC_API_KEY - OAuth preferred
OAuth should be preferred  
✅ Using Claude.ai OAuth authentication
```

### Network Request Failing
```
starting new connection: https://claude.ai/
connected to [2607:6bc0::10]:443
parsed 23 headers
incoming body is content-length (20082 bytes) // HTML PAGE, NOT API
```

## JavaScript Tool Behavior Analysis

### What JavaScript Tool ACTUALLY Does (Found in code)
1. **OAuth Endpoints Found**: 
   - `https://api.anthropic.com/api/oauth/claude_cli/create_api_key`
   - `https://api.anthropic.com/api/oauth/claude_cli/roles`

2. **Messages Endpoint Usage**:
   - `this._client.post("/v1/messages")` (line 372201)
   - Goes through a client object that sets base URL

3. **Scope Issue**: 
   - Conversion endpoint requires `org:create_api_key` scope
   - User tokens only have `user:inference` + `user:profile`

## Critical Unknown Factors

1. **How does JavaScript tool get `org:create_api_key` scope?**
   - Different OAuth flow?
   - Different endpoint?
   - Different authentication method entirely?

2. **What is the actual base URL for JavaScript client when using OAuth?**
   - Does it still use api.anthropic.com?
   - Does it use a different endpoint entirely?
   - Does it use WebSocket or other non-HTTP protocol?

3. **Is claude.ai actually used for API calls?**
   - 20KB HTML response suggests no
   - But JavaScript has claude.ai references
   - Might be for different functionality

## Files Modified
- `src/auth.rs`: Complete rewrite of authentication logic
- `src/ai/mod.rs`: Added OAuth support to AIConfig  
- `src/ai/client.rs`: Added Bearer authentication
- `SPECS/JS_AUTHENTICATION_RESEARCH.md`: Updated with findings

## Immediate Next Steps Needed

1. **STOP GUESSING** - Find actual evidence of how JavaScript tool works
2. **Determine if JavaScript tool uses api.anthropic.com or different endpoint**
3. **Find how JavaScript tool gets required OAuth scopes**  
4. **Test if OAuth tokens work with api.anthropic.com using different headers/approach**

## Time Spent
Over 5 hours of debugging with no working solution.

## User Frustration Level
EXTREMELY HIGH - user wants to use Max subscription instead of API credits and has been blocked for hours.

---

**IMPORTANT**: The next agent should focus on finding ACTUAL evidence in the JavaScript code rather than making assumptions about endpoints or authentication methods.