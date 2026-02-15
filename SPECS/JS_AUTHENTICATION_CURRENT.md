# JavaScript Authentication - CURRENT ANALYSIS

## CRITICAL DISCOVERY: OAUTH TO API KEY EXCHANGE

The JavaScript tool DOES NOT use OAuth tokens directly as Bearer tokens or API keys!

## The Real Authentication Flow

### 1. OAuth Token Exchange Process
When OAuth is detected, the JavaScript tool:
1. Has OAuth access token from Claude Desktop (`sk-ant-oat01-...`)
2. Calls `https://api.anthropic.com/api/oauth/claude_cli/create_api_key` with the OAuth token
3. Gets back a real API key in the response (`data.raw_key`)
4. Uses that API key with `x-api-key` header for all subsequent API calls

### 2. Key Endpoints (from line 341012)
```javascript
{
    API_KEY_URL: "https://api.anthropic.com/api/oauth/claude_cli/create_api_key",
    ROLES_URL: "https://api.anthropic.com/api/oauth/claude_cli/roles",
    BASE_API_URL: "https://api.anthropic.com",
    CLIENT_ID: "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
}
```

### 3. API Key Creation (line 355319)
```javascript
// Call the create_api_key endpoint with OAuth token
let config8199 = await axios.post(API_KEY_URL, {
    headers: {
        "Authorization": `Bearer ${oauth_token}`
    }
});
let next2170 = config8199.data?.raw_key;  // The actual API key
if (next2170) {
    TvA(next2170);  // Store the API key
    return next2170;
}
```

### 4. Client Creation After Exchange
Once the API key is obtained:
```javascript
let input20337 = {
    apiKey: api_key_from_exchange,  // Real API key from exchange
    authToken: undefined,
    ...options1339,
};
return new Ow(input20337);
```

### 5. Headers Sent to API
With the exchanged API key:
- Sends `x-api-key: {exchanged_api_key}` header
- Sends `x-app: cli` header
- Base URL is `https://api.anthropic.com/v1`

## THE SOLUTION

Our Rust tool needs to:
1. Detect OAuth tokens (they start with `sk-ant-oat01-`)
2. Call `https://api.anthropic.com/api/oauth/claude_cli/create_api_key` with `Authorization: Bearer {oauth_token}`
3. Extract the `raw_key` from the response
4. Use that key as the `x-api-key` for all API calls

## Why Our Current Approach Fails

We were trying to use the OAuth token directly as:
- Bearer token: API says "OAuth authentication is currently not supported"
- x-api-key: API says "invalid x-api-key" (because it's not a real API key)

The OAuth token (`sk-ant-oat01-...`) is NOT an API key - it's an OAuth access token that must be exchanged for a real API key!

## Key Functions

- `checker53()`: Returns true when OAuth is available
- `UZ()`: Returns OAuth credentials from Claude Desktop
- `MS()`: Returns API key from apiKeyHelper
- `QX()`: Gets API key from environment
- `MK()`: Creates the API client
- `Ow`: The actual API client class
- `func255()`: Adds auth headers (NOT called when OAuth is active)
- `BB()`: Returns OAuth configuration URLs

## OAuth Token Source
The OAuth tokens come from Claude Desktop, stored in keychain with:
- Service: "Claude Code-credentials"
- Contains: `claudeAiOauth` object with `accessToken`, `refreshToken`, `expiresAt`, `scopes`
- Scopes: `["user:inference", "user:profile"]`

## The Bottom Line

The JavaScript sends Bearer tokens to the API when OAuth is detected, and IT WORKS.
Our Rust implementation does the same thing, but gets rejected with "OAuth authentication is currently not supported".

There is something the JavaScript tool is doing that we haven't discovered yet.