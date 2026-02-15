// ============================================================================
// EXTRACTED AUTHENTICATION CODE FROM cli-jsdef-fixed.js
// ============================================================================
// This file contains ALL authentication-related code with original line numbers
// for easy reference to the source JavaScript file.
//
// Created: 2025-01-XX
// Purpose: Never search the 270k line JS file for auth code again!
// ============================================================================

// ============================================================================
// SECTION 1: OAuth Constants & Configuration
// Lines 66728-66772
// ============================================================================

// OAuth scopes and beta header constants
var variable4301 = "user:inference", variable10135 = "org:create_api_key", variable29407 = "oauth-2025-04-20", variable20470, variable8308, variable8259, variable34635, variable13653 = undefined, variable592;
var variable31683 = ((variable22124, variable6404)=>{
    return ()=>(variable22124 && (variable6404 = variable22124(variable22124 = 0)), variable6404);
})((()=>{
    variable35191();
    // Console OAuth scopes (for creating API keys)
    variable20470 = [
        variable10135,
        "user:profile"
    ],
    // Claude.ai OAuth scopes (full access including inference)
    variable8308 = [
        "user:profile",
        variable4301,
        "user:sessions:claude_code",
        ...[]
    ],
    // All scopes combined
    variable8259 = Array.from(new Set([
        ...variable20470,
        ...variable8308
    ])),
    // PRODUCTION OAuth configuration
    variable34635 = {
        BASE_API_URL: "https://api.anthropic.com",
        CONSOLE_AUTHORIZE_URL: "https://console.anthropic.com/oauth/authorize",
        CLAUDE_AI_AUTHORIZE_URL: "https://claude.ai/oauth/authorize",
        TOKEN_URL: "https://console.anthropic.com/v1/oauth/token",
        API_KEY_URL: "https://api.anthropic.com/api/oauth/claude_cli/create_api_key",
        ROLES_URL: "https://api.anthropic.com/api/oauth/claude_cli/roles",
        CONSOLE_SUCCESS_URL: "https://console.anthropic.com/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dclaude-code",
        CLAUDEAI_SUCCESS_URL: "https://console.anthropic.com/oauth/code/success?app=claude-code",
        MANUAL_REDIRECT_URL: "https://console.anthropic.com/oauth/code/callback",
        CLIENT_ID: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
        OAUTH_FILE_SUFFIX: "",
        MCP_PROXY_URL: undefined,
        MCP_PROXY_PATH: undefined
    },
    // LOCAL DEVELOPMENT OAuth configuration
    variable592 = {
        BASE_API_URL: "http://localhost:3000",
        CONSOLE_AUTHORIZE_URL: "http://localhost:3000/oauth/authorize",
        CLAUDE_AI_AUTHORIZE_URL: "http://localhost:4000/oauth/authorize",
        TOKEN_URL: "http://localhost:3000/v1/oauth/token",
        API_KEY_URL: "http://localhost:3000/api/oauth/claude_cli/create_api_key",
        ROLES_URL: "http://localhost:3000/api/oauth/claude_cli/roles",
        CONSOLE_SUCCESS_URL: "http://localhost:3000/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dclaude-code",
        CLAUDEAI_SUCCESS_URL: "http://localhost:3000/oauth/code/success?app=claude-code",
        MANUAL_REDIRECT_URL: "https://console.staging.ant.dev/oauth/code/callback",
        CLIENT_ID: "22422756-60c9-4084-8eb7-27705fd5cf9a",
        OAUTH_FILE_SUFFIX: "-local-oauth",
        MCP_PROXY_URL: "http://localhost:8205",
        MCP_PROXY_PATH: "/v1/toolbox/shttp/mcp/{server_id}"
    };
}));

// ============================================================================
// SECTION 2: Storage Backend
// Lines 70951-70954
// ============================================================================

// Get keychain/storage backend (macOS uses keychain, others use JSON file)
function variable35391() {
    if (process.platform === "darwin") return variable34766(variable23160, variable21385);
    return variable21385;
}

// ============================================================================
// SECTION 3: Scope Checking & Parsing
// Lines 71054-71059
// ============================================================================

// Check if scopes include the inference scope
function variable11754(variable22124) {
    return Boolean(variable22124?.includes(variable4301));
}

// Parse space-separated scope string into array
function variable16124(variable22124) {
    return variable22124?.split(" ").filter(Boolean) ?? [];
}

// ============================================================================
// SECTION 4: OAuth Authorization Flow - Build URL
// Lines 71060-71068
// ============================================================================

// Build OAuth authorization URL with PKCE parameters
function variable39614({ codeChallenge: variable22124, state: variable6404, port: variable29010, isManual: variable21016, loginWithClaudeAi: variable26452, inferenceOnly: variable27524, orgUUID: variable1729 }) {
    let variable17475 = variable26452 ? variable14954().CLAUDE_AI_AUTHORIZE_URL : variable14954().CONSOLE_AUTHORIZE_URL, variable16850 = new URL(variable17475);
    variable16850.searchParams.append("code", "true"), variable16850.searchParams.append("client_id", variable14954().CLIENT_ID), variable16850.searchParams.append("response_type", "code"), variable16850.searchParams.append("redirect_uri", variable21016 ? variable14954().MANUAL_REDIRECT_URL : `http://localhost:${variable29010}/callback`);
    let variable37445 = variable27524 ? [
        variable4301
    ] : variable8259;
    if (variable16850.searchParams.append("scope", variable37445.join(" ")), variable16850.searchParams.append("code_challenge", variable22124), variable16850.searchParams.append("code_challenge_method", "S256"), variable16850.searchParams.append("state", variable6404), variable1729) variable16850.searchParams.append("orgUUID", variable1729);
    return variable16850.toString();
}

// ============================================================================
// SECTION 5: OAuth Authorization Flow - Token Exchange
// Lines 71069-71086
// ============================================================================

// Exchange authorization code for tokens (PKCE flow)
async function variable6758(variable22124, variable6404, variable29010, variable21016, variable26452 = false, variable27524) {
    let variable1729 = {
        grant_type: "authorization_code",
        code: variable22124,
        redirect_uri: variable26452 ? variable14954().MANUAL_REDIRECT_URL : `http://localhost:${variable21016}/callback`,
        client_id: variable14954().CLIENT_ID,
        code_verifier: variable29010,
        state: variable6404
    };
    if (variable27524 !== undefined) variable1729.expires_in = variable27524;
    let variable17475 = await variable11574.post(variable14954().TOKEN_URL, variable1729, {
        headers: {
            "Content-Type": "application/json"
        }
    });
    if (variable17475.status !== 200) throw Error(variable17475.status === 401 ? "Authentication failed: Invalid authorization code" : `Token exchange failed (${variable17475.status}): ${variable17475.statusText}`);
    return variable2248("tengu_oauth_token_exchange_success", {}), variable17475.data;
}

// ============================================================================
// SECTION 6: OAuth Profile Fetching
// Lines 71032-71044
// ============================================================================

// Fetch OAuth profile from /api/oauth/profile
async function variable10301(variable22124) {
    let variable6404 = `${variable14954().BASE_API_URL}/api/oauth/profile`;
    try {
        return (await variable11574.get(variable6404, {
            headers: {
                Authorization: `Bearer ${variable22124}`,
                "Content-Type": "application/json"
            }
        })).data;
    } catch (variable29010) {
        variable23718(variable29010);
    }
}

// ============================================================================
// SECTION 7: OAuth Token Refresh
// Lines 71087-71129
// ============================================================================

// Refresh OAuth token using refresh_token grant
async function variable27603(variable22124) {
    let variable6404 = {
        grant_type: "refresh_token",
        refresh_token: variable22124,
        client_id: variable14954().CLIENT_ID,
        scope: variable8308.join(" ")
    };
    try {
        let variable29010 = await variable11574.post(variable14954().TOKEN_URL, variable6404, {
            headers: {
                "Content-Type": "application/json"
            }
        });
        if (variable29010.status !== 200) throw Error(`Token refresh failed: ${variable29010.statusText}`);
        let variable21016 = variable29010.data, { access_token: variable26452, refresh_token: variable27524 = variable22124, expires_in: variable1729 } = variable21016, variable17475 = Date.now() + variable1729 * 1000, variable16850 = variable16124(variable21016.scope);
        variable2248("tengu_oauth_token_refresh_success", {});
        let variable37445 = await variable3554(variable26452);
        if (variable36311().oauthAccount) {
            let variable30042 = {};
            if (variable37445.displayName !== undefined) variable30042.displayName = variable37445.displayName;
            if (typeof variable37445.hasExtraUsageEnabled === "boolean") variable30042.hasExtraUsageEnabled = variable37445.hasExtraUsageEnabled;
            if (Object.keys(variable30042).length > 0) variable37151(((variable19732)=>({
                    ...variable19732,
                    oauthAccount: variable19732.oauthAccount ? {
                        ...variable19732.oauthAccount,
                        ...variable30042
                    } : variable19732.oauthAccount
                })));
        }
        return {
            accessToken: variable26452,
            refreshToken: variable27524,
            expiresAt: variable17475,
            scopes: variable16850,
            subscriptionType: variable37445.subscriptionType,
            rateLimitTier: variable37445.rateLimitTier
        };
    } catch (variable29010) {
        throw variable2248("tengu_oauth_token_refresh_failure", {
            error: variable29010.message
        }), variable29010;
    }
}

// ============================================================================
// SECTION 8: User Roles Fetching
// Lines 71130-71150
// ============================================================================

// Fetch user roles from OAuth API
async function variable26344(variable22124) {
    let variable6404 = await variable11574.get(variable14954().ROLES_URL, {
        headers: {
            Authorization: `Bearer ${variable22124}`
        }
    });
    if (variable6404.status !== 200) throw Error(`Failed to fetch user roles: ${variable6404.statusText}`);
    let variable29010 = variable6404.data;
    if (!variable36311().oauthAccount) throw Error("OAuth account information not found in config");
    variable37151(((variable26452)=>({
            ...variable26452,
            oauthAccount: variable26452.oauthAccount ? {
                ...variable26452.oauthAccount,
                organizationRole: variable29010.organization_role,
                workspaceRole: variable29010.workspace_role,
                organizationName: variable29010.organization_name
            } : variable26452.oauthAccount
        }))), variable2248("tengu_oauth_roles_stored", {
        org_role: variable29010.organization_role
    });
}

// ============================================================================
// SECTION 9: API Key Creation from OAuth
// Lines 71151-71169
// ============================================================================

// Create API key from OAuth token (used by /login)
async function variable32572(variable22124) {
    try {
        let variable6404 = await variable11574.post(variable14954().API_KEY_URL, null, {
            headers: {
                Authorization: `Bearer ${variable22124}`
            }
        }), variable29010 = variable6404.data?.raw_key;
        if (variable29010) return variable1666(variable29010), variable2248("tengu_oauth_api_key", {
            status: "success",
            statusCode: variable6404.status
        }), variable29010;
        return null;
    } catch (variable6404) {
        throw variable2248("tengu_oauth_api_key", {
            status: "failure",
            error: variable6404 instanceof Error ? variable6404.message : String(variable6404)
        }), variable6404;
    }
}

// ============================================================================
// SECTION 10: Token Expiry Checking
// Lines 71170-71174
// ============================================================================

// Check if token is expiring (within 5 minutes)
function variable25259(variable22124) {
    if (variable22124 === null) return false;
    let variable6404 = 300000;
    return Date.now() + 300000 >= variable22124;
}

// ============================================================================
// SECTION 11: Subscription Type from Profile
// Lines 71175-71201
// ============================================================================

// Get subscription type from OAuth profile
async function variable3554(variable22124) {
    let variable6404 = await variable10301(variable22124), variable29010 = variable6404?.organization?.organization_type, variable21016 = null;
    switch(variable29010){
        case "claude_max":
            variable21016 = "max";
            break;
        case "claude_pro":
            variable21016 = "pro";
            break;
        case "claude_enterprise":
            variable21016 = "enterprise";
            break;
        case "claude_team":
            variable21016 = "team";
            break;
        default:
            variable21016 = null;
            break;
    }
    let variable26452 = {
        subscriptionType: variable21016,
        rateLimitTier: variable6404?.organization?.rate_limit_tier ?? null,
        hasExtraUsageEnabled: variable6404?.organization?.has_extra_usage_enabled ?? null
    };
    if (variable6404?.account?.display_name) variable26452.displayName = variable6404.account.display_name;
    return variable2248("tengu_oauth_profile_fetch_success", {}), variable26452;
}

// ============================================================================
// SECTION 12: Organization UUID Retrieval
// Lines 71202-71210
// ============================================================================

// Get organization UUID from cached config or OAuth profile
async function variable7410() {
    let variable6404 = variable36311().oauthAccount?.organizationUuid;
    if (variable6404) return variable6404;
    let variable29010 = variable13907()?.accessToken;
    if (variable29010 === undefined) return null;
    let variable26452 = (await variable10301(variable29010))?.organization?.uuid;
    if (!variable26452) return null;
    return variable26452;
}

// ============================================================================
// SECTION 13: Ensure OAuth Account Stored
// Lines 71211-71225
// ============================================================================

// Ensure OAuth account info is fetched and stored in config
async function variable34056() {
    if (variable36311().oauthAccount || !variable22847()) return false;
    let variable6404 = variable13907();
    if (variable6404?.accessToken) {
        let variable29010 = await variable10301(variable6404.accessToken);
        if (variable29010) return variable16218({
            accountUuid: variable29010.account.uuid,
            emailAddress: variable29010.account.email,
            organizationUuid: variable29010.organization.uuid,
            displayName: variable29010.account.display_name || undefined,
            hasExtraUsageEnabled: variable29010.organization.has_extra_usage_enabled ?? false
        }), true;
    }
    return false;
}

// ============================================================================
// SECTION 14: Store OAuth Account in Config
// Lines 71226-71241
// ============================================================================

// Store OAuth account information in global config
function variable16218({ accountUuid: variable22124, emailAddress: variable6404, organizationUuid: variable29010, displayName: variable21016, hasExtraUsageEnabled: variable26452 }) {
    let variable27524 = {
        accountUuid: variable22124,
        emailAddress: variable6404,
        organizationUuid: variable29010,
        hasExtraUsageEnabled: variable26452
    };
    if (variable21016) variable27524.displayName = variable21016;
    variable37151(((variable1729)=>{
        if (variable1729.oauthAccount?.accountUuid === variable27524.accountUuid && variable1729.oauthAccount?.emailAddress === variable27524.emailAddress && variable1729.oauthAccount?.organizationUuid === variable27524.organizationUuid && variable1729.oauthAccount?.displayName === variable27524.displayName && variable1729.oauthAccount?.hasExtraUsageEnabled === variable27524.hasExtraUsageEnabled) return variable1729;
        return {
            ...variable1729,
            oauthAccount: variable27524
        };
    }));
}

// ============================================================================
// SECTION 15: API Key Storage - Delete from Keychain
// Lines 134114-134119
// ============================================================================

// Delete API key from macOS keychain
function variable285() {
    if (process.platform === "darwin") {
        let variable22124 = variable5759();
        variable20472(`security delete-generic-password -a $USER -s "${variable22124}"`);
    }
}

// ============================================================================
// SECTION 16: Auth Token Source Detection
// Lines 134187-134213
// ============================================================================

// Get auth token source with priority order
function variable27871() {
    if (process.env.ANTHROPIC_AUTH_TOKEN) return {
        source: "ANTHROPIC_AUTH_TOKEN",
        hasToken: true
    };
    if (process.env.CLAUDE_CODE_OAUTH_TOKEN) return {
        source: "CLAUDE_CODE_OAUTH_TOKEN",
        hasToken: true
    };
    if (variable39284()) return {
        source: "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
        hasToken: true
    };
    if (variable19321()) return {
        source: "apiKeyHelper",
        hasToken: true
    };
    let variable29010 = variable13907();
    if (variable11754(variable29010?.scopes) && variable29010?.accessToken) return {
        source: "claude.ai",
        hasToken: true
    };
    return {
        source: "none",
        hasToken: false
    };
}

// ============================================================================
// SECTION 17: API Key Retrieval - Simple Wrapper
// Lines 134214-134217
// ============================================================================

// Get current API key (wrapper around variable29576)
function variable27302() {
    let { key: variable22124 } = variable29576();
    return variable22124;
}

// ============================================================================
// SECTION 18: API Key Retrieval - Full Priority Chain
// Lines 134224-134272
// ============================================================================

// Get API key with full priority chain and source tracking
function variable29576(variable22124 = {}) {
    if (variable21634() && process.env.ANTHROPIC_API_KEY) return {
        key: process.env.ANTHROPIC_API_KEY,
        source: "ANTHROPIC_API_KEY"
    };
    if (variable27155(false)) {
        let variable21016 = variable23437();
        if (variable21016) return {
            key: variable21016,
            source: "ANTHROPIC_API_KEY"
        };
        if (!process.env.ANTHROPIC_API_KEY && !process.env.CLAUDE_CODE_OAUTH_TOKEN && !process.env.CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR) throw Error("ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN env var is required");
        if (process.env.ANTHROPIC_API_KEY) return {
            key: process.env.ANTHROPIC_API_KEY,
            source: "ANTHROPIC_API_KEY"
        };
        return {
            key: null,
            source: "none"
        };
    }
    if (process.env.ANTHROPIC_API_KEY && variable36311().customApiKeyResponses?.approved?.includes(variable11437(process.env.ANTHROPIC_API_KEY))) return {
        key: process.env.ANTHROPIC_API_KEY,
        source: "ANTHROPIC_API_KEY"
    };
    let variable6404 = variable23437();
    if (variable6404) return {
        key: variable6404,
        source: "ANTHROPIC_API_KEY"
    };
    if (variable22124.skipRetrievingKeyFromApiKeyHelper) {
        if (variable19321()) return {
            key: null,
            source: "apiKeyHelper"
        };
    } else {
        let variable21016 = variable7760(variable29236());
        if (variable21016) return {
            key: variable21016,
            source: "apiKeyHelper"
        };
    }
    let variable29010 = variable35813();
    if (variable29010) return variable29010;
    return {
        key: null,
        source: "none"
    };
}

// ============================================================================
// SECTION 19: API Key Validation
// Lines 134418-134420
// ============================================================================

// Validate API key format (alphanumeric, dashes, underscores only)
function variable27880(variable22124) {
    return /^[a-zA-Z0-9-_]+$/.test(variable22124);
}

// ============================================================================
// SECTION 20: API Key Storage - Save to Keychain
// Lines 134421-134458
// ============================================================================

// Save API key to keychain (macOS) or config file
function variable1666(variable22124) {
    if (!variable27880(variable22124)) throw Error("Invalid API key format. API key must contain only alphanumeric characters, dashes, and underscores.");
    variable36150();
    let variable6404 = false;
    if (process.platform === "darwin") try {
        let variable21016 = variable5759(), variable26452 = variable36759(), variable27524 = Buffer.from(variable22124, "utf-8").toString("hex"), variable1729 = `add-generic-password -U -a "${variable26452}" -s "${variable21016}" -X "${variable27524}"
`;
        variable20472("security -i", {
            input: variable1729,
            stdio: [
                "pipe",
                "pipe",
                "pipe"
            ]
        }), variable2248("tengu_api_key_saved_to_keychain", {}), variable6404 = true;
    } catch (variable21016) {
        variable23718(variable21016), variable2248("tengu_api_key_keychain_error", {
            error: variable21016.message
        }), variable2248("tengu_api_key_saved_to_config", {});
    }
    else variable2248("tengu_api_key_saved_to_config", {});
    let variable29010 = variable11437(variable22124);
    variable37151(((variable21016)=>{
        let variable26452 = variable21016.customApiKeyResponses?.approved ?? [];
        return {
            ...variable21016,
            primaryApiKey: variable6404 ? variable21016.primaryApiKey : variable22124,
            customApiKeyResponses: {
                ...variable21016.customApiKeyResponses,
                approved: variable26452.includes(variable29010) ? variable26452 : [
                    ...variable26452,
                    variable29010
                ],
                rejected: variable21016.customApiKeyResponses?.rejected ?? []
            }
        };
    })), variable35813.cache.clear?.();
}

// ============================================================================
// SECTION 21: API Key Storage - Clear Primary Key
// Lines 134459-134464
// ============================================================================

// Clear primary API key from config
function variable27444() {
    variable36150(), variable37151(((variable22124)=>({
            ...variable22124,
            primaryApiKey: undefined
        }))), variable35813.cache.clear?.();
}

// ============================================================================
// SECTION 22: API Key Storage - Clear from Storage
// Lines 134465-134471
// ============================================================================

// Clear API key from storage (calls keychain delete)
function variable36150() {
    try {
        variable285();
    } catch (variable22124) {
        variable23718(variable22124);
    }
}

// ============================================================================
// SECTION 23: OAuth Token Storage
// Lines 134472-134507
// ============================================================================

// Save OAuth tokens to storage backend
function variable28018(variable22124) {
    if (!variable11754(variable22124.scopes)) return variable2248("tengu_oauth_tokens_not_claude_ai", {}), {
        success: true
    };
    if (!variable22124.refreshToken || !variable22124.expiresAt) return variable2248("tengu_oauth_tokens_inference_only", {}), {
        success: true
    };
    let variable6404 = variable35391(), variable29010 = variable6404.name;
    try {
        let variable21016 = variable6404.read() || {};
        variable21016.claudeAiOauth = {
            accessToken: variable22124.accessToken,
            refreshToken: variable22124.refreshToken,
            expiresAt: variable22124.expiresAt,
            scopes: variable22124.scopes,
            subscriptionType: variable22124.subscriptionType,
            rateLimitTier: variable22124.rateLimitTier
        };
        let variable26452 = variable6404.update(variable21016);
        if (variable26452.success) variable2248("tengu_oauth_tokens_saved", {
            storageBackend: variable29010
        });
        else variable2248("tengu_oauth_tokens_save_failed", {
            storageBackend: variable29010
        });
        return variable13907.cache?.clear?.(), variable20063(), variable26452;
    } catch (variable21016) {
        return variable23718(variable21016), variable2248("tengu_oauth_tokens_save_exception", {
            storageBackend: variable29010,
            error: variable21016.message
        }), {
            success: false,
            warning: "Failed to save OAuth tokens"
        };
    }
}

// ============================================================================
// SECTION 24: OAuth Token Refresh with Locking
// Lines 134508-134541
// ============================================================================

// Refresh OAuth token with file-based locking (prevents race conditions)
async function variable820(variable22124 = 0) {
    let variable29010 = variable13907();
    if (!variable29010?.refreshToken || !variable25259(variable29010.expiresAt)) return false;
    if (!variable11754(variable29010.scopes)) return false;
    if (variable13907.cache?.clear?.(), variable29010 = variable13907(), !variable29010?.refreshToken || !variable25259(variable29010.expiresAt)) return false;
    let variable21016 = variable27957();
    variable29155().mkdirSync(variable21016);
    let variable27524;
    try {
        variable2248("tengu_oauth_token_refresh_lock_acquiring", {}), variable27524 = await variable4719.lock(variable21016), variable2248("tengu_oauth_token_refresh_lock_acquired", {});
    } catch (variable1729) {
        if (variable1729.code === "ELOCKED") {
            if (variable22124 < 5) return variable2248("tengu_oauth_token_refresh_lock_retry", {
                retryCount: variable22124 + 1
            }), await new Promise(((variable17475)=>setTimeout(variable17475, 1000 + Math.random() * 1000))), variable820(variable22124 + 1);
            return variable2248("tengu_oauth_token_refresh_lock_retry_limit_reached", {
                maxRetries: 5
            }), false;
        }
        return variable23718(variable1729), variable2248("tengu_oauth_token_refresh_lock_error", {
            error: variable1729.message
        }), false;
    }
    try {
        if (variable13907.cache?.clear?.(), variable29010 = variable13907(), !variable29010?.refreshToken || !variable25259(variable29010.expiresAt)) return variable2248("tengu_oauth_token_refresh_race_resolved", {}), false;
        variable2248("tengu_oauth_token_refresh_starting", {});
        let variable1729 = await variable27603(variable29010.refreshToken);
        return variable28018(variable1729), variable13907.cache?.clear?.(), true;
    } catch (variable1729) {
        return variable23718(variable1729 instanceof Error ? variable1729 : Error(String(variable1729))), false;
    } finally{
        variable2248("tengu_oauth_token_refresh_lock_releasing", {}), await variable27524(), variable2248("tengu_oauth_token_refresh_lock_released", {});
    }
}

// ============================================================================
// SECTION 25: Check if Using OAuth
// Lines 134542-134545
// ============================================================================

// Check if currently using Claude.ai OAuth authentication
function variable22847() {
    if (!variable10519()) return false;
    return variable11754(variable13907()?.scopes);
}

// ============================================================================
// SECTION 26: OAuth Token Retrieval (Memoized)
// Lines 134636-134746
// ============================================================================

// CRITICAL: Memoized OAuth token retrieval
// Reads from env vars or keychain storage
// Returns: { accessToken, refreshToken, expiresAt, scopes, subscriptionType, rateLimitTier }
var variable4719, variable7156 = 300000, variable7760, variable23712 = 3600000, variable24279, variable35813, variable13907;
var variable27686 = ((variable22124, variable6404)=>{
    return ()=>(variable22124 && (variable6404 = variable22124(variable22124 = 0)), variable6404);
})((()=>{
    // ... initialization code ...
    variable4719 = variable23648(variable24428(), 1);
    // ... other initializations ...

    // OAUTH TOKEN RETRIEVAL - MEMOIZED
    variable13907 = variable23124((()=>{
        // First priority: CLAUDE_CODE_OAUTH_TOKEN env var (inference-only)
        if (process.env.CLAUDE_CODE_OAUTH_TOKEN) return {
            accessToken: process.env.CLAUDE_CODE_OAUTH_TOKEN,
            refreshToken: null,
            expiresAt: null,
            scopes: [
                "user:inference"
            ],
            subscriptionType: null,
            rateLimitTier: null
        };
        // Second priority: File descriptor
        let variable22124 = variable39284();
        if (variable22124) return {
            accessToken: variable22124,
            refreshToken: null,
            expiresAt: null,
            scopes: [
                "user:inference"
            ],
            subscriptionType: null,
            rateLimitTier: null
        };
        // Third priority: Keychain/storage (full OAuth with refresh token)
        try {
            let variable21016 = variable35391().read()?.claudeAiOauth;
            if (!variable21016?.accessToken) return null;
            return variable21016;
        } catch (variable6404) {
            return variable23718(variable6404), null;
        }
    }));
}));

// ============================================================================
// SECTION 27: CREATE CLIENT - CRITICAL OAuth vs API Key Logic
// Lines 272469-272565
// ============================================================================

// Logger for SDK
function variable31026() {
    return {
        error: (variable22124, ...variable6404)=>console.error("[Anthropic SDK ERROR]", variable22124, ...variable6404),
        warn: (variable22124, ...variable6404)=>console.error("[Anthropic SDK WARN]", variable22124, ...variable6404),
        info: (variable22124, ...variable6404)=>console.error("[Anthropic SDK INFO]", variable22124, ...variable6404),
        debug: (variable22124, ...variable6404)=>console.error("[Anthropic SDK DEBUG]", variable22124, ...variable6404)
    };
}

// CRITICAL: Create Anthropic Client
// This is where OAuth vs API key authentication is decided
async function variable32270({ apiKey: variable22124, maxRetries: variable6404, model: variable29010, fetchOverride: variable21016 }) {
    let variable26452 = process.env.CLAUDE_CODE_CONTAINER_ID, variable27524 = process.env.CLAUDE_CODE_REMOTE_SESSION_ID, variable1729 = {
        "x-app": "cli",
        "User-Agent": variable22811(),
        ...variable22963(),
        ...variable26452 ? {
            "x-claude-remote-container-id": variable26452
        } : {},
        ...variable27524 ? {
            "x-claude-remote-session-id": variable27524
        } : {}
    };
    if (variable27155(process.env.CLAUDE_CODE_ADDITIONAL_PROTECTION)) variable1729["x-anthropic-additional-protection"] = "true";

    // CRITICAL: Refresh OAuth token, then set Authorization header if NOT OAuth
    if (await variable820(), !variable22847()) variable32350(variable1729, variable29236());

    let variable16850 = {
        defaultHeaders: variable1729,
        maxRetries: variable6404,
        timeout: parseInt(process.env.API_TIMEOUT_MS || String(600000), 10),
        dangerouslyAllowBrowser: true,
        fetchOptions: variable35933(),
        ...variable21016 && {
            fetch: variable21016
        }
    };

    // [Bedrock, Vertex, Foundry client creation omitted for brevity]

    // *** CRITICAL OAuth vs API Key Decision ***
    // This is the DEFAULT Anthropic client creation
    // When variable22847() (isUsingOAuth) returns true:
    //   - apiKey is set to NULL
    //   - authToken is set to the OAuth access token
    // When NOT using OAuth:
    //   - apiKey is set to the API key (from param or from storage)
    //   - authToken is UNDEFINED
    let variable37445 = {
        apiKey: variable22847() ? null : variable22124 || variable27302(),
        authToken: variable22847() ? variable13907()?.accessToken : undefined,
        ...{},
        ...variable16850,
        ...variable25919() && {
            logger: variable31026()
        }
    };
    return new variable4200(variable37445);
}

// Set Authorization header from ANTHROPIC_AUTH_TOKEN or apiKeyHelper
// ONLY called when NOT using OAuth (see line 272482)
function variable32350(variable22124, variable6404) {
    let variable29010 = process.env.ANTHROPIC_AUTH_TOKEN || variable7760(variable6404);
    if (variable29010) variable22124.Authorization = `Bearer ${variable29010}`;
}

// Get custom headers from ANTHROPIC_CUSTOM_HEADERS env var
function variable22963() {
    let variable22124 = {}, variable6404 = process.env.ANTHROPIC_CUSTOM_HEADERS;
    if (!variable6404) return variable22124;
    let variable29010 = variable6404.split(/\n|\r\n/);
    for (let variable21016 of variable29010){
        if (!variable21016.trim()) continue;
        let variable26452 = variable21016.match(/^\s*(.*?)\s*:\s*(.*?)\s*$/);
        if (variable26452) {
            let [, variable27524, variable1729] = variable26452;
            variable22124[variable27524] = variable1729;
        }
    }
    return variable22124;
}

// ============================================================================
// SECTION 28: Request Header Helpers - OAuth Headers
// Lines 141779-141803
// ============================================================================

// Get auth headers for OAuth requests
function variable19365() {
    if (variable22847()) {
        let variable6404 = variable13907();
        if (!variable6404?.accessToken) return {
            headers: {},
            error: "No OAuth token available"
        };
        return {
            headers: {
                Authorization: `Bearer ${variable6404.accessToken}`,
                "anthropic-beta": variable29407
            }
        };
    }
    let variable22124 = variable27302();
    if (!variable22124) return {
        headers: {},
        error: "No API key available"
    };
    return {
        headers: {
            "x-api-key": variable22124
        }
    };
}

// ============================================================================
// SECTION 28: Anthropic Client - Constructor & Auth Methods
// Lines 191553-191620
// ============================================================================

// Anthropic SDK Client Constructor with Auth
constructor({ baseURL: variable22124 = variable34900("ANTHROPIC_BASE_URL"), apiKey: variable6404 = variable34900("ANTHROPIC_API_KEY") ?? null, authToken: variable29010 = variable34900("ANTHROPIC_AUTH_TOKEN") ?? null, ...variable21016 } = {}){
    variable27784.add(this), variable31605.set(this, undefined);
    let variable26452 = {
        apiKey: variable6404,
        authToken: variable29010,
        ...variable21016,
        baseURL: variable22124 || "https://api.anthropic.com"
    };
    if (!variable26452.dangerouslyAllowBrowser && variable9064()) throw new variable10185(`It looks like you're running in a browser-like environment.

This is disabled by default, as it risks exposing your secret API credentials to attackers.
If you understand the risks and have appropriate mitigations in place,
you can set the \`dangerouslyAllowBrowser\` option to \`true\`, e.g.,

new Anthropic({ apiKey, dangerouslyAllowBrowser: true });
`);
    this.baseURL = variable26452.baseURL, this.timeout = variable26452.timeout ?? variable18850.DEFAULT_TIMEOUT, this.logger = variable26452.logger ?? console;
    let variable27524 = "warn";
    this.logLevel = variable27524, this.logLevel = variable23971(variable26452.logLevel, "ClientOptions.logLevel", this) ?? variable23971(variable34900("ANTHROPIC_LOG"), "process.env['ANTHROPIC_LOG']", this) ?? variable27524, this.fetchOptions = variable26452.fetchOptions, this.maxRetries = variable26452.maxRetries ?? 2, this.fetch = variable26452.fetch ?? variable1902(), variable16437(this, variable31605, variable30027, "f"), this._options = variable26452, this.apiKey = typeof variable6404 === "string" ? variable6404 : null, this.authToken = variable29010;
}

// Validate that auth headers are set
validateHeaders({ values: variable22124, nulls: variable6404 }) {
    if (variable22124.get("x-api-key") || variable22124.get("authorization")) return;
    if (this.apiKey && variable22124.get("x-api-key")) return;
    if (variable6404.has("x-api-key")) return;
    if (this.authToken && variable22124.get("authorization")) return;
    if (variable6404.has("authorization")) return;
    throw Error('Could not resolve authentication method. Expected either apiKey or authToken to be set. Or for one of the "X-Api-Key" or "Authorization" headers to be explicitly omitted');
}

// Combine API key and Bearer auth
async authHeaders(variable22124) {
    return variable30891([
        await this.apiKeyAuth(variable22124),
        await this.bearerAuth(variable22124)
    ]);
}

// API Key authentication (X-Api-Key header)
async apiKeyAuth(variable22124) {
    if (this.apiKey == null) return;
    return variable30891([
        {
            "X-Api-Key": this.apiKey
        }
    ]);
}

// Bearer token authentication (Authorization header)
async bearerAuth(variable22124) {
    if (this.authToken == null) return;
    return variable30891([
        {
            Authorization: `Bearer ${this.authToken}`
        }
    ]);
}

// ============================================================================
// SECTION 29: Request Header Helpers - Bearer Headers
// Lines 302080-302086
// ============================================================================

// Create Bearer auth headers for API requests
function variable28234(variable22124) {
    return {
        Authorization: `Bearer ${variable22124}`,
        "Content-Type": "application/json",
        "anthropic-version": "2023-06-01"
    };
}

// ============================================================================
// SECTION 30: OpenTelemetry Flush (Pre-Logout)
// Lines 392755-392778
// ============================================================================

// Flush OpenTelemetry telemetry before logout
async function variable35268() {
    let variable22124 = variable15448();
    if (!variable22124) return;
    let variable6404 = parseInt(process.env.CLAUDE_CODE_OTEL_FLUSH_TIMEOUT_MS || "5000");
    try {
        let variable29010 = [
            variable22124.forceFlush()
        ], variable21016 = variable14275();
        if (variable21016) variable29010.push(variable21016.forceFlush());
        let variable26452 = variable15491();
        if (variable26452) variable29010.push(variable26452.forceFlush());
        await Promise.race([
            Promise.all(variable29010),
            new Promise(((variable27524, variable1729)=>setTimeout((()=>variable1729(Error("OpenTelemetry flush timeout"))), variable6404)))
        ]), variable2092("Telemetry flushed successfully");
    } catch (variable29010) {
        if (variable29010 instanceof Error && variable29010.message.includes("timeout")) variable2092(`Telemetry flush timed out after ${variable6404}ms. Some metrics may not be exported.`, {
            level: "warn"
        });
        else variable2092(`Telemetry flush failed: ${variable29010 instanceof Error ? variable29010.message : String(variable29010)}`, {
            level: "error"
        });
    }
}

// ============================================================================
// SECTION 31: Logout & Cleanup
// Lines 393421-393437
// ============================================================================

// Logout function - clears all auth state
async function variable4329({ clearOnboarding: variable22124 = false }) {
    await variable35268(), variable27444(), variable35391().delete(), variable20836(), variable37151(((variable29010)=>{
        let variable21016 = {
            ...variable29010
        };
        if (variable22124) {
            if (variable21016.hasCompletedOnboarding = false, variable21016.subscriptionNoticeCount = 0, variable21016.hasAvailableSubscription = false, variable21016.customApiKeyResponses?.approved) variable21016.customApiKeyResponses = {
                ...variable21016.customApiKeyResponses,
                approved: []
            };
        }
        return variable21016.oauthAccount = undefined, variable21016;
    }));
}

// Clear all auth caches
var variable13198, variable20836 = ()=>{
    variable13907.cache?.clear?.(), variable20063(), variable22975(), variable28666(), variable39008.cache?.clear?.(), variable5290.cache?.clear?.(), variable13488();
}, variable39792;

// ============================================================================
// END OF EXTRACTED AUTHENTICATION CODE
// ============================================================================
