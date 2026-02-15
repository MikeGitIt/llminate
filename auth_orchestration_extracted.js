// Authentication Orchestration Functions Extracted from test-fixed.js
// These are the key functions that handle authentication flow and orchestration

// ====== ANTHROPIC CLIENT AUTHENTICATION SETUP ======

// Main Anthropic client class constructor - handles multiple auth methods
class AnthropicClient {
  constructor({
    baseURL = Qt("ANTHROPIC_BASE_URL"),
    apiKey = Qt("ANTHROPIC_API_KEY") ?? null,
    authToken = Qt("ANTHROPIC_AUTH_TOKEN") ?? null,
    ...additionalOptions
  } = {}) {
    let config = {
      apiKey: apiKey,
      authToken: authToken,
      ...additionalOptions,
      baseURL: baseURL || "https://api.anthropic.com",
    };

    if (!config.dangerouslyAllowBrowser && isBrowserEnvironment())
      throw new Error(
        "It looks like you're running in a browser-like environment.\n\nThis is disabled by default, as it risks exposing your secret API credentials to attackers.\nIf you understand the risks and have appropriate mitigations in place,\nyou can set the `dangerouslyAllowBrowser` option to `true`, e.g.,\n\nnew Anthropic({ apiKey, dangerouslyAllowBrowser: true });\n",
      );

    this.baseURL = config.baseURL;
    this.timeout = config.timeout ?? DEFAULT_TIMEOUT;
    this.logger = config.logger ?? console;
  }
}

// ====== AUTHENTICATION METHOD VALIDATION ======

// Header validation function - checks which auth method is available
function validateHeaders({ values, nulls }) {
  if (this.apiKey && values.get("x-api-key")) return;
  if (nulls.has("x-api-key")) return;
  if (this.authToken && values.get("authorization")) return;
  if (nulls.has("authorization")) return;
  throw new Error(
    'Could not resolve authentication method. Expected either apiKey or authToken to be set. Or for one of the "X-Api-Key" or "Authorization" headers to be explicitly omitted',
  );
}

// ====== AUTHENTICATION HEADER BUILDERS ======

// API Key authentication headers
function authHeaders() {
  if (this.apiKey == null) return;
  return buildHeaders([
    {
      "X-Api-Key": this.apiKey,
    },
  ]);
}

// Bearer token authentication headers
function authTokenHeaders() {
  if (this.authToken == null) return;
  return buildHeaders([
    {
      Authorization: `Bearer ${this.authToken}`,
    },
  ]);
}

// ====== AUTHENTICATION TOKEN SETUP ======

// Sets up Authorization header with Bearer token
function setupBearerAuth(headers) {
  let authToken = process.env.ANTHROPIC_AUTH_TOKEN || getStoredToken();
  if (authToken) {
    headers.Authorization = `Bearer ${authToken}`;
    headers["Proxy-Authorization"] = `Bearer ${authToken}`;
  }
}

// ====== API KEY VALIDATION AND EXTRACTION ======

// Main API key resolution function - checks multiple sources
function resolveApiKey(requireKey) {
  if (requireKey && process.env.ANTHROPIC_API_KEY)
    return {
      key: process.env.ANTHROPIC_API_KEY,
      source: "ANTHROPIC_API_KEY",
    };

  if (process.env.ANTHROPIC_API_KEY &&
      getConfig().customApiKeyResponses?.approved?.includes(
        maskApiKey(process.env.ANTHROPIC_API_KEY),
      ))
    return {
      key: process.env.ANTHROPIC_API_KEY,
      source: "ANTHROPIC_API_KEY",
    };

  let storedKey = getStoredApiKey();
  if (storedKey)
    return {
      key: storedKey,
      source: "apiKeyHelper",
    };

  let alternateKey = getAlternateKey();
  if (alternateKey) return alternateKey;

  return {
    key: null,
    source: "none",
  };
}

// ====== OAUTH TOKEN MANAGEMENT ======

// OAuth token refresh functionality
async function refreshAccessToken() {
  var accessToken;
  let currentToken = (await this.authClient.getAccessToken()).token;
  let tokenRequest = {
    grantType: "urn:ietf:params:oauth:grant-type:token-exchange",
    requestedTokenType: "urn:ietf:params:oauth:token-type:access_token",
    subjectToken: currentToken,
    subjectTokenType: "urn:ietf:params:oauth:token-type:access_token",
  };

  let exchangeResult = await this.stsCredential.exchangeToken(
    tokenRequest,
    undefined,
    this.credentialAccessBoundary,
  );

  let expiryDate =
    ((accessToken = this.authClient.credentials) === null ||
    accessToken === undefined
      ? undefined
      : accessToken.expiry_date) || null;

  let newExpiryTime = exchangeResult.expires_in
    ? new Date().getTime() + exchangeResult.expires_in * 1000
    : expiryDate;

  return {
    cachedDownscopedAccessToken: {
      access_token: exchangeResult.access_token,
      expiry_date: newExpiryTime,
    }
  };
}

// ====== AUTHENTICATION STATE CHECKING ======

// Checks if authentication should use OAuth flow
function shouldUseOAuthFlow() {
  let useAlternateProvider =
      process.env.CLAUDE_CODE_USE_BEDROCK || process.env.CLAUDE_CODE_USE_VERTEX;
  let authToken = process.env.ANTHROPIC_AUTH_TOKEN || getStoredAuthHelper();
  let { source } = resolveApiKey(getModelConfig());

  return !(
    useAlternateProvider ||
    authToken ||
    source === "ANTHROPIC_API_KEY" ||
    source === "apiKeyHelper"
  );
}

// Checks current authentication token status
function checkAuthTokenStatus() {
  if (process.env.ANTHROPIC_AUTH_TOKEN)
    return {
      source: "ANTHROPIC_AUTH_TOKEN",
      hasToken: true,
    };
  if (getStoredToken())
    return {
      source: "storedToken",
      hasToken: true,
    };
  return {
    source: "none",
    hasToken: false,
  };
}

// ====== HTTP AUTH SCHEME CONFIGURATION ======

// Default HTTP authentication scheme provider
async function defaultHttpAuthSchemeProvider(authSchemeParameters) {
  let availableSchemes = [];

  // Check for API key authentication
  if (authSchemeParameters.apiKey) {
    availableSchemes.push({
      schemeId: "apiKey",
      identityProperties: {},
      signingProperties: {
        apiKey: authSchemeParameters.apiKey,
      },
    });
  }

  // Check for Bearer token authentication
  if (authSchemeParameters.authToken) {
    availableSchemes.push({
      schemeId: "bearer",
      identityProperties: {},
      signingProperties: {
        token: authSchemeParameters.authToken,
      },
    });
  }

  return availableSchemes;
}

// Resolve HTTP authentication configuration
function resolveHttpAuthSchemeConfig(options) {
  let stsConfig = resolveStsAuthConfig(options);
  let sigV4Config = resolveAwsSdkSigV4Config(stsConfig);

  return Object.assign(sigV4Config, {
    authSchemePreference: normalizeProvider(
      options.authSchemePreference ?? [],
    ),
  });
}

// ====== BEARER TOKEN SIGNERS ======

// HTTP Bearer token authentication signer
class HttpBearerAuthSigner {
  async sign(request, config, next) {
    let signedRequest = HttpRequest.clone(request);
    if (!config.token)
      throw new Error(
        "request could not be signed with `token` since the `token` is not defined",
      );

    signedRequest.headers.Authorization = `Bearer ${config.token}`;
    return signedRequest;
  }
}

// ====== AUTHENTICATION ORCHESTRATION MAIN FUNCTION ======

// Main authentication orchestration function
async function setupAuthentication(clientConfig) {
  // 1. Check environment and determine auth method
  let useOAuth = shouldUseOAuthFlow();
  let tokenStatus = checkAuthTokenStatus();
  let apiKeyInfo = resolveApiKey(clientConfig.requireApiKey);

  // 2. Setup authentication based on available methods
  if (tokenStatus.hasToken) {
    // Use OAuth/Bearer token authentication
    return {
      authType: "bearer",
      headers: {
        Authorization: `Bearer ${tokenStatus.token}`,
      },
    };
  } else if (apiKeyInfo.key) {
    // Use API key authentication
    return {
      authType: "apiKey",
      headers: {
        "X-Api-Key": apiKeyInfo.key,
      },
    };
  } else if (useOAuth) {
    // Initiate OAuth flow
    return {
      authType: "oauth",
      requiresFlow: true,
    };
  } else {
    // No authentication available
    throw new Error("No valid authentication method found");
  }
}

// ====== CUSTOM HEADER PARSING ======

// Parse custom headers from environment
function parseCustomHeaders() {
  let headers = {};
  let customHeadersEnv = process.env.ANTHROPIC_CUSTOM_HEADERS;
  if (!customHeadersEnv) return headers;

  let headerLines = customHeadersEnv.split(/\n|\r\n/);
  for (let line of headerLines) {
    let separatorIndex = line.indexOf(":");
    if (separatorIndex !== -1) {
      let headerName = line.slice(0, separatorIndex);
      let headerValue = line.slice(separatorIndex + 1, line.length);
      if (validateKey(headerName) && validateValue(headerValue))
        headers[headerName] = headerValue;
    }
  }
  return headers;
}

// ====== RETRY CONFIGURATION FOR AUTH ======

// Setup retry configuration for authentication failures
function setupAuthRetryConfig() {
  return {
    retryDelay: 1000,
    retryCount: 0,
    maxRetries: 3,
    retryableStatusCodes: [401, 403, 429],
    backoffStrategy: "exponential",
  };
}

// Refresh retry token for authentication retry
async function refreshRetryTokenForRetry(request, errorInfo) {
  await this.rateLimiter.getSendToken();
  return this.standardRetryStrategy.acquireInitialRetryToken(request);
}

// ====== ENVIRONMENT VALIDATION ======

// Validate environment for authentication setup
function validateEnvironmentId() {
  var environmentId;
  let envId =
    (environmentId = this.environmentId) === null || environmentId === undefined
      ? undefined
      : environmentId;
  if (!envId) {
    throw new Error("Environment ID is required for authentication setup");
  }
  return envId;
}

// ====== API KEY APPROVAL CHECKING ======

// Check if API key is in approved list
function isApiKeyApproved(apiKey) {
  let config = getConfig();
  let maskedKey = maskApiKey(apiKey);
  return config.customApiKeyResponses?.approved?.includes(maskedKey);
}

// Mask API key for security (show only last 4 characters)
function maskApiKey(apiKey) {
  if (!apiKey || apiKey.length < 8) return apiKey;
  return apiKey.slice(0, 7) + "..." + apiKey.slice(-4);
}

// ====== ERROR HANDLING FOR AUTHENTICATION ======

// Handle authentication errors and provide user-friendly messages
function handleAuthError(error, context) {
  if (error instanceof Error &&
      error.message.toLowerCase().includes("x-api-key")) {
    let { source } = resolveApiKey(context);
    return {
      content: source === "ANTHROPIC_API_KEY" || source === "apiKeyHelper"
        ? "API key authentication failed - please check your key"
        : "Custom API key authentication failed",
    };
  }

  if (error.status === 403 &&
      error.message.includes("OAuth token has been revoked")) {
    return {
      content: "OAuth token has been revoked - please re-authenticate",
    };
  }

  return {
    content: "Authentication failed - please check your credentials",
  };
}

// ====== CONFIGURATION CONSTANTS ======

const AUTH_CONFIG = {
  ANTHROPIC_API: {
    BASE_API_URL: "https://api.anthropic.com",
    CONSOLE_AUTHORIZE_URL: "https://console.anthropic.com/oauth/authorize",
    CLAUDE_AI_AUTHORIZE_URL: "https://claude.ai/oauth/authorize",
    TOKEN_URL: "https://console.anthropic.com/v1/oauth/token",
    API_KEY_URL: "https://api.anthropic.com/api/oauth/claude_cli/create_api_key",
    ROLES_URL: "https://api.anthropic.com/api/oauth/claude_cli/roles",
    CLIENT_ID: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
  },
  DEFAULT_TIMEOUT: 300000,
  SCOPES: ["org:create_api_key", "user:profile", "user:inference"],
};

// Export all authentication functions
module.exports = {
  AnthropicClient,
  validateHeaders,
  authHeaders,
  authTokenHeaders,
  setupBearerAuth,
  resolveApiKey,
  refreshAccessToken,
  shouldUseOAuthFlow,
  checkAuthTokenStatus,
  defaultHttpAuthSchemeProvider,
  resolveHttpAuthSchemeConfig,
  HttpBearerAuthSigner,
  setupAuthentication,
  parseCustomHeaders,
  setupAuthRetryConfig,
  refreshRetryTokenForRetry,
  validateEnvironmentId,
  isApiKeyApproved,
  maskApiKey,
  handleAuthError,
  AUTH_CONFIG,
};