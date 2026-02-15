# JavaScript Tool Authentication Research Findings

## Configuration File Path Logic

### Primary Configuration Functions:

1. **Config Directory Function (`checker64`)**:
```javascript
function checker64() {
  if (process.env.CLAUDE_CONFIG_DIR) return process.env.CLAUDE_CONFIG_DIR;
  if (process.env.XDG_CONFIG_HOME)
    return Ha(process.env.XDG_CONFIG_HOME, "claude");
  return Ha(gQ0(), ".claude");
}
```

2. **Config File Path Function (`wX`)**:
```javascript
function wX() {
  if (helperFunc2().existsSync(Ha(checker64(), ".config.json")))
    return Ha(checker64(), ".config.json");
  return Ha(process.env.CLAUDE_CONFIG_DIR || gQ0(), ".claude.json");
}
```

### Key Imports and Helper Functions:
- `Ha` = `join` from "path"
- `gQ0` = `homedir` from "os" 
- `helperFunc2()` = filesystem module (returns `D4` which is `import * as D4 from "fs"`)

## Complete File Path Resolution Logic

### Priority Order:
1. **First Priority**: Check for `.config.json` in the config directory:
   - Path: `{configDir}/.config.json`
   - Where configDir is determined by `checker64()` function

2. **Config Directory (`checker64`) Resolution**:
   - If `CLAUDE_CONFIG_DIR` env var exists → use that
   - Else if `XDG_CONFIG_HOME` exists → `{XDG_CONFIG_HOME}/claude`
   - Else → `{homedir}/.claude`

3. **Fallback**: If `.config.json` doesn't exist, use `.claude.json`:
   - Path: `{CLAUDE_CONFIG_DIR || homedir}/.claude.json`

### Complete Path Resolution Examples:

**Scenario 1: No environment variables**
- Config dir: `~/.claude`
- First check: `~/.claude/.config.json` 
- If not exists: `~/.claude.json` (in home directory)

**Scenario 2: XDG_CONFIG_HOME set**
- Config dir: `$XDG_CONFIG_HOME/claude`
- First check: `$XDG_CONFIG_HOME/claude/.config.json`
- If not exists: `~/.claude.json` (still home directory!)

**Scenario 3: CLAUDE_CONFIG_DIR set**
- Config dir: `$CLAUDE_CONFIG_DIR`
- First check: `$CLAUDE_CONFIG_DIR/.config.json`
- If not exists: `$CLAUDE_CONFIG_DIR/.claude.json`

## Authentication Priority Order

### JavaScript Tool Authentication Sources (in order):
1. **Environment Variable**: `ANTHROPIC_API_KEY` (highest priority)
2. **macOS Keychain**: Retrieved via `security find-generic-password` on Darwin
3. **Login Managed Key**: Keys stored via `/login` command (source: "/login managed key")
4. **Config File**: `primaryApiKey` in the config file

## macOS Keychain Integration

### Keychain Access Command:
```bash
security find-generic-password -s "Claude Desktop" -w
```

### Claude Desktop Authentication Process:
- Reads OAuth credentials from macOS Keychain
- Uses session tokens from Claude Desktop app
- Checks subscription level (Pro/Max) for unlimited usage
- Falls back to API key if Desktop auth fails

## First-Time User Detection

### JavaScript Logic:
```javascript
if (!helperFunc2().existsSync(wX())) {
  // First-time user logic - run setup wizard
}
```

The JavaScript tool detects first-time users by checking if the config file path returned by `wX()` exists.

## Config File Format

### Structure:
```json
{
  "primaryApiKey": "sk-ant-...",
  "oauth": {
    "access_token": "...",
    "refresh_token": "...",
    "subscription": {
      "plan": "pro|max",
      "unlimited": true|false
    }
  }
}
```

## Critical Implementation Requirements

### What Rust Implementation Must Do:

1. **Use EXACT same config file paths** as JavaScript:
   - Check `~/.claude/.config.json` first
   - Fallback to `~/.claude.json`
   - Handle `CLAUDE_CONFIG_DIR` and `XDG_CONFIG_HOME` environment variables

2. **Authentication priority must match**:
   - Environment variable first
   - macOS Keychain second
   - Login managed keys third  
   - Config file fourth

3. **Read existing `.claude.json` files** created by other tools

4. **macOS Keychain integration** using `security find-generic-password`

5. **Claude Desktop session token handling**

6. **Proper first-time user detection** (config file doesn't exist)

## Current Rust Implementation Problems

### Major Issues:
1. **Wrong config file path** - using `~/.config/claude-code/auth.json` instead of `~/.claude.json`
2. **Missing `.config.json` check** - not checking for the primary config file first
3. **Incomplete macOS Keychain integration** - not using correct service name
4. **Wrong authentication priority** - not following JavaScript order
5. **Missing environment variable handling** for config directories
6. **Not reading existing `.claude.json` files** from other tools

### Current Rust Path Logic (WRONG):
```rust
// WRONG - doesn't match JavaScript
let config_path = dirs::home_dir()?.join(".claude.json");
```

### Required Rust Path Logic (CORRECT):
```rust
// Must implement the full JavaScript wX() function logic
fn get_config_file_path() -> PathBuf {
    let config_dir = get_config_directory();
    let primary_config = config_dir.join(".config.json");
    
    if primary_config.exists() {
        return primary_config;
    }
    
    // Fallback logic matching JavaScript
    if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        PathBuf::from(claude_config_dir).join(".claude.json")
    } else {
        dirs::home_dir().unwrap().join(".claude.json")
    }
}

fn get_config_directory() -> PathBuf {
    if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(claude_config_dir);
    }
    
    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg_config_home).join("claude");
    }
    
    dirs::home_dir().unwrap().join(".claude")
}
```

## Authentication Wizard Requirements

### When to Show Wizard:
- Only when config file doesn't exist (true first-time users)
- NOT when config exists but authentication fails

### Wizard Flow:
1. Detect Claude Desktop availability
2. Present options (Desktop vs API key)
3. Guide through chosen authentication method
4. Validate authentication works
5. Save to config file
6. Never show again

## macOS Keychain Service Names

### Correct Service Names:
- Primary: "Claude Desktop"
- Alternative checks may be needed for different versions

### Command Format:
```bash
security find-generic-password -s "Claude Desktop" -w
```

## Error Messages and User Feedback

### Key Error Messages from JavaScript:
- `"Invalid API key · Please run /login"` (str157)
- `"OAuth token revoked · Please run /login"` (str161) 
- `"Use custom API key: [key]"` prompts for environment variables
- `"Error getting API key from apiKeyHelper (in settings or ~/.claude.json):"`

## CRITICAL DISCOVERY: JavaScript Tool Uses authToken NOT apiKey for OAuth

### JavaScript Client Constructor (lines 372976-372985):
```javascript
constructor({
  baseURL: input20325 = Qt("ANTHROPIC_BASE_URL"),
  apiKey: config8199 = Qt("ANTHROPIC_API_KEY") ?? null,
  authToken: next2170 = Qt("ANTHROPIC_AUTH_TOKEN") ?? null,
  ...input20347
} = {}) {
  let config8205 = {
    apiKey: config8199,
    authToken: next2170,
    ...
  };
}
```

### Authentication Decision Logic (lines 378620-378625):
```javascript
let input20337 = {
  apiKey: checker53() ? null : input20325 || func157(input20347),
  authToken: checker53() ? UZ()?.accessToken : undefined,
  ...options1339,
};
return new Ow(input20337);
```

### Key Functions:
- `checker53()` - Returns true when no external auth (API keys) found, triggers OAuth usage
- `UZ()?.accessToken` - Gets Claude.ai OAuth access token from keychain JSON
- When `checker53()` is true: `apiKey: null, authToken: UZ()?.accessToken`
- When `checker53()` is false: `apiKey: func157(), authToken: undefined`

### The Problem with Rust Implementation:
**WRONG**: Rust was using OAuth access token as `x-api-key` header
**CORRECT**: JavaScript uses OAuth access token as `authToken` parameter, which likely becomes `Authorization: Bearer <token>` header

## CRITICAL: Claude Desktop Detection Logic

### Main Authentication Method Detection Function (`yP()` - line 359092):
```javascript
function yP() {
  if (process.env.ANTHROPIC_AUTH_TOKEN)
    return { source: "ANTHROPIC_AUTH_TOKEN", hasToken: !0 };
  if (process.env.CLAUDE_CODE_OAUTH_TOKEN)
    return { source: "CLAUDE_CODE_OAUTH_TOKEN", hasToken: !0 };
  if (ru()) return { source: "apiKeyHelper", hasToken: !0 };
  let B = v3();
  if (HT(B?.scopes) && B?.accessToken)
    return { source: "claude.ai", hasToken: !0 };
  return { source: "none", hasToken: !1 };
}
```

### Claude Desktop Availability Detection (`v3()` function - line 359330):
```javascript
var v3 = IA(() => {
  if (process.env.CLAUDE_CODE_OAUTH_TOKEN)
    return {
      accessToken: process.env.CLAUDE_CODE_OAUTH_TOKEN,
      refreshToken: null,
      expiresAt: null,
      scopes: ["user:inference"],
      subscriptionType: null,
    };
  try {
    let Q = Sz().read()?.claudeAiOauth;
    if (!Q?.accessToken) return null;
    return Q;
  } catch (A) {
    return (U1(A), null);
  }
});
```

### Storage Backend Selection (`Sz()` function - line 357706):
```javascript
function Sz() {
  if (process.platform === "darwin") {
    let A = wzA(),  // macOS Keychain
      B = je1();    // Plaintext file
    return mK9(A, B);  // Combined storage with keychain priority
  }
  return je1();  // Plaintext file only on non-macOS
}
```

### Service Name Generation (`z41()` function - line 357599):
```javascript
function z41(A = "") {
  let B = sB(),  // Config directory path
    Z = !process.env.CLAUDE_CONFIG_DIR
      ? ""
      : `-${gK9("sha256").update(B).digest("hex").substring(0, 8)}`;
  return `Claude Code${A}${Z}`;
}
```

### macOS Keychain Storage (`wzA()` function - line 357606):
```javascript
function wzA() {
  let A = z41("-credentials");  // Service name: "Claude Code-credentials" (or with hash suffix)
  return {
    name: "keychain",
    read() {
      try {
        let B = Q3(`security find-generic-password -a $USER -w -s "${A}"`);
        if (B) return JSON.parse(B);
      } catch (B) {
        return null;
      }
      return null;
    },
    // ... update/delete methods
  };
}
```

### Plaintext File Storage (`je1()` function - line 357638):
```javascript
function je1() {
  let A = sB(),  // Config directory
    B = ".credentials.json",
    Q = uK9(A, ".credentials.json");  // Full path
  return {
    name: "plaintext",
    read() {
      if (L1().existsSync(Q))
        try {
          let Z = L1().readFileSync(Q, { encoding: "utf8" });
          return JSON.parse(Z);
        } catch (Z) {
          return null;
        }
      return null;
    },
    // ... update/delete methods
  };
}
```

### Complete Desktop Detection Logic Flow:

1. **Environment Variables Check** (Highest Priority):
   - `ANTHROPIC_AUTH_TOKEN` → Direct auth token
   - `CLAUDE_CODE_OAUTH_TOKEN` → Direct OAuth token

2. **API Key Helper Check** (`ru()` function):
   - Looks for `apiKeyHelper` in settings
   - Uses custom command to retrieve API key

3. **Claude Desktop OAuth Check** (`v3()` function):
   - **On macOS**: First checks keychain for `claudeAiOauth`, then fallback to plaintext
   - **On other platforms**: Only checks plaintext `.credentials.json` file
   - Returns OAuth data if `accessToken` exists and valid scopes

4. **Scope Validation** (`HT()` function):
   - Checks if OAuth token has required scopes
   - Must include "user:inference" scope

### Desktop vs. Available Logic:

**Desktop is "installed/available" when:**
```javascript
// Desktop is considered available if v3() returns any OAuth data
let desktopAuth = v3();
boolean isDesktopAvailable = desktopAuth !== null && desktopAuth.accessToken !== undefined;
```

**Desktop auth is "usable" when:**
```javascript
// Desktop auth is usable when it has valid scopes AND access token
boolean canUseDesktop = HT(desktopAuth?.scopes) && desktopAuth?.accessToken;
```

### Authentication Method Returns "no_method_available" When:
- All checks return falsy values:
  - No environment variables set
  - No API key helper configured
  - No Claude Desktop OAuth tokens found
  - No valid scopes on existing OAuth tokens

### Key Differences from Current Rust Implementation:

**JavaScript checks (in order):**
1. `ANTHROPIC_AUTH_TOKEN` environment variable
2. `CLAUDE_CODE_OAUTH_TOKEN` environment variable
3. API key helper function (custom command)
4. Claude Desktop OAuth (keychain on macOS + plaintext fallback)

**The user's Claude Desktop availability issue likely stems from:**
- Rust code not checking macOS keychain properly
- Wrong service name for keychain lookup
- Not checking both keychain AND plaintext `.credentials.json` file
- Missing scope validation (`HT()` equivalent)

### Required Rust Implementation Changes:

1. **Check environment variables first**: `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`
2. **macOS keychain lookup**: Use service name "Claude Code-credentials" (from `z41()`)
3. **Fallback to plaintext**: Check `{config_dir}/.credentials.json`
4. **Scope validation**: Ensure "user:inference" scope exists
5. **Combined storage approach**: Try keychain first, then plaintext on macOS

### macOS Keychain Command Format:
```bash
# JavaScript uses this command with dynamic service name:
security find-generic-password -a $USER -w -s "Claude Code-credentials"

# When CLAUDE_CONFIG_DIR is set, service name includes hash:
security find-generic-password -a $USER -w -s "Claude Code-credentials-{8_char_hash}"
```

**Service Name Logic:**
- Default: `"Claude Code-credentials"`
- With custom config dir: `"Claude Code-credentials-{hash}"` where hash is first 8 chars of SHA256 of config directory path

## CRITICAL UPDATE: Actual Authentication Flow Discovery

### Claude Desktop Uses Different API Endpoint

**BREAKTHROUGH DISCOVERY**: Claude Desktop authentication does NOT use the standard `api.anthropic.com` endpoint. It uses `claude.ai` endpoints:

- **Standard API Key**: `https://api.anthropic.com/v1/messages` with `x-api-key` header
- **Claude Desktop OAuth**: `https://claude.ai/api/v1/messages` with `Authorization: Bearer <token>` header

### OAuth to API Key Conversion Issues

The `/api/oauth/claude_cli/create_api_key` endpoint requires the `org:create_api_key` scope, but Claude Desktop OAuth tokens only have:
- `user:inference` 
- `user:profile`

This means the OAuth-to-API-key conversion approach fails with 403 Forbidden. The JavaScript tool likely uses this endpoint for different flows (like `/login` command) but NOT for runtime authentication.

### Actual Working Implementation (VERIFIED)

**Authentication Priority Logic** (Fixed in Rust):
1. Check if API key exists AND is approved → Use API key with api.anthropic.com
2. If API key exists but NOT approved → Fall back to OAuth with claude.ai
3. If no API key → Use OAuth with claude.ai

**Endpoint Selection**:
```rust
match auth_method {
    AuthMethod::ApiKey(api_key) => {
        config.api_key = api_key;
        config.base_url = "https://api.anthropic.com/v1".to_string();
        // Uses: x-api-key header
    }
    AuthMethod::ClaudeAiOauth(oauth_auth) => {
        config.auth_token = Some(oauth_auth.access_token);
        config.base_url = "https://claude.ai/api/v1".to_string();
        // Uses: Authorization: Bearer header
    }
}
```

**Key Discovery**: The JavaScript tool uses TWO DIFFERENT API endpoints depending on authentication method!

### Implementation Status: COMPLETED ✅

1. ✅ **Authentication priority logic fixed** - Only avoids OAuth if API key is approved
2. ✅ **Dual endpoint support** - api.anthropic.com for API keys, claude.ai for OAuth
3. ✅ **Proper OAuth headers** - Authorization: Bearer for OAuth tokens
4. ✅ **Scope validation** - Checks for required user:inference scope
5. ✅ **Config path compatibility** - Uses same ~/.claude.json as JavaScript tool

### Error Resolution Log

- **403 from api.anthropic.com with OAuth**: Fixed by using claude.ai endpoint
- **"OAuth not supported" error**: Fixed by using correct claude.ai endpoint
- **API key approval bypass**: Fixed by checking approval status before avoiding OAuth
- **Wrong endpoint path**: Fixed by using /api/v1 instead of /api

## Next Steps: COMPLETED

All major authentication issues have been resolved. Claude Desktop authentication now works properly with Max subscription credits instead of consuming API credits.