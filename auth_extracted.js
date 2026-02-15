/**
 * Authentication Functions Extracted from test-fixed.js
 * This file contains all authentication-related functions and their dependencies
 * extracted from the obfuscated JavaScript codebase.
 */

// =============================================================================
// HTTP AUTHENTICATION INFRASTRUCTURE
// =============================================================================

// Authentication Location Constants
const HttpApiKeyAuthLocation = {
  HEADER: "header",
  QUERY: "query"
};

// HTTP Request Class with cloning support
class HttpRequest {
  constructor(options) {
    this.method = options.method || "GET";
    this.hostname = options.hostname || "localhost";
    this.port = options.port;
    this.query = options.query || {};
    this.headers = options.headers || {};
    this.body = options.body;
    this.protocol = options.protocol
      ? options.protocol.slice(-1) !== ":"
        ? `${options.protocol}:`
        : options.protocol
      : "https:";
    this.path = options.path
      ? options.path.charAt(0) !== "/"
        ? `/${options.path}`
        : options.path
      : "/";
    this.username = options.username;
    this.password = options.password;
    this.fragment = options.fragment;
  }
  
  static clone(request) {
    let cloned = new HttpRequest({
      ...request,
      headers: {
        ...request.headers,
      },
    });
    if (cloned.query) {
      cloned.query = cloneQuery(cloned.query);
    }
    return cloned;
  }
  
  static isInstance(obj) {
    if (!obj) return false;
    let request = obj;
    return (
      "method" in request &&
      "protocol" in request &&
      "hostname" in request &&
      "path" in request &&
      typeof request.query === "object" &&
      typeof request.headers === "object"
    );
  }
  
  clone() {
    return HttpRequest.clone(this);
  }
}

function cloneQuery(query) {
  return Object.keys(query).reduce((result, key) => {
    let value = query[key];
    return {
      ...result,
      [key]: Array.isArray(value) ? [...value] : value,
    };
  }, {});
}

// Default Identity Provider Configuration
class DefaultIdentityProviderConfig {
  constructor(authSchemes) {
    this.authSchemes = new Map();
    for (let [schemeId, provider] of Object.entries(authSchemes)) {
      if (provider !== undefined) {
        this.authSchemes.set(schemeId, provider);
      }
    }
  }
  
  getIdentityProvider(schemeId) {
    return this.authSchemes.get(schemeId);
  }
}

// HTTP API Key Authentication Signer
class HttpApiKeyAuthSigner {
  async sign(request, identity, signingProperties) {
    if (!signingProperties) {
      throw new Error(
        "request could not be signed with `apiKey` since the `name` and `in` signer properties are missing"
      );
    }
    if (!signingProperties.name) {
      throw new Error(
        "request could not be signed with `apiKey` since the `name` signer property is missing"
      );
    }
    if (!signingProperties.in) {
      throw new Error(
        "request could not be signed with `apiKey` since the `in` signer property is missing"
      );
    }
    if (!identity.apiKey) {
      throw new Error(
        "request could not be signed with `apiKey` since the `apiKey` is not defined"
      );
    }
    
    let clonedRequest = HttpRequest.clone(request);
    
    if (signingProperties.in === HttpApiKeyAuthLocation.QUERY) {
      clonedRequest.query[signingProperties.name] = identity.apiKey;
    } else if (signingProperties.in === HttpApiKeyAuthLocation.HEADER) {
      clonedRequest.headers[signingProperties.name] = signingProperties.scheme
        ? `${signingProperties.scheme} ${identity.apiKey}`
        : identity.apiKey;
    } else {
      throw new Error(
        "request can only be signed with `apiKey` locations `query` or `header`, but found: `" +
          signingProperties.in +
          "`"
      );
    }
    
    return clonedRequest;
  }
}

// HTTP Bearer Token Authentication Signer
class HttpBearerAuthSigner {
  async sign(request, identity, signingProperties) {
    let clonedRequest = HttpRequest.clone(request);
    
    if (!identity.token) {
      throw new Error(
        "request could not be signed with `token` since the `token` is not defined"
      );
    }
    
    clonedRequest.headers.Authorization = `Bearer ${identity.token}`;
    return clonedRequest;
  }
}

// No Authentication Signer (pass-through)
class NoAuthSigner {
  async sign(request, identity, signingProperties) {
    return request;
  }
}

// =============================================================================
// ANTHROPIC API AUTHENTICATION
// =============================================================================

// Configuration Constants
const ANTHROPIC_CONFIG = {
  BASE_API_URL: "https://api.anthropic.com",
  CONSOLE_AUTHORIZE_URL: "https://console.anthropic.com/oauth/authorize",
  CLAUDE_AI_AUTHORIZE_URL: "https://claude.ai/oauth/authorize",
  OAUTH_SCOPE: "user:inference",
  OAUTH_BETA_HEADER: "oauth-2025-04-20"
};

// Environment Variable Reader
const getEnvVariable = (varName) => {
  if (typeof globalThis.process !== "undefined")
    return globalThis.process.env?.[varName]?.trim() ?? undefined;
  if (typeof globalThis.Deno !== "undefined")
    return globalThis.Deno.env?.get?.(varName)?.trim();
  return undefined;
};

// API Key Truncation (for display)
function truncateApiKey(apiKey) {
  return apiKey.slice(-20);
}

// Main API Key Resolution Function
function resolveAnthropicApiKey(forceEnv) {
  // Check environment variable first
  if (forceEnv && process.env.ANTHROPIC_API_KEY) {
    return {
      key: process.env.ANTHROPIC_API_KEY,
      source: "ANTHROPIC_API_KEY",
    };
  }
  
  // Check if environment variable is approved
  if (process.env.ANTHROPIC_API_KEY && 
      isApiKeyApproved(process.env.ANTHROPIC_API_KEY)) {
    return {
      key: process.env.ANTHROPIC_API_KEY,
      source: "ANTHROPIC_API_KEY",
    };
  }
  
  // Check API key helper
  let helperKey = getApiKeyFromHelper();
  if (helperKey) {
    return {
      key: helperKey,
      source: "apiKeyHelper",
    };
  }
  
  // Check managed keys (platform-specific)
  let managedKey = getManagedApiKey();
  if (managedKey) {
    return managedKey;
  }
  
  return {
    key: null,
    source: "none",
  };
}

// Token Availability Check
function checkTokenAvailability() {
  if (process.env.ANTHROPIC_AUTH_TOKEN) {
    return {
      source: "ANTHROPIC_AUTH_TOKEN",
      hasToken: true,
    };
  }
  
  if (getApiKeyFromHelper()) {
    return {
      source: "apiKeyHelper",
      hasToken: true,
    };
  }
  
  let oauthToken = getOAuthToken();
  if (oauthToken && hasValidScopes(oauthToken)) {
    return {
      source: "claude.ai",
      hasToken: true,
    };
  }
  
  return {
    source: "none",
    hasToken: false,
  };
}

// Simple API Key Getter
function getAnthropicApiKey(forceEnv) {
  let { key } = resolveAnthropicApiKey(forceEnv);
  return key;
}

// Bearer Token Setup for Headers
function setupBearerToken(headers) {
  let token = process.env.ANTHROPIC_AUTH_TOKEN || getApiKeyFromHelper();
  if (token) {
    headers.Authorization = `Bearer ${token}`;
    headers["Proxy-Authorization"] = `Bearer ${token}`;
  }
}

// Custom Headers from Environment
function getCustomHeaders() {
  let headers = {};
  let customHeaders = process.env.ANTHROPIC_CUSTOM_HEADERS;
  
  if (!customHeaders) return headers;
  
  let lines = customHeaders.split(/\n|\r\n/);
  for (let line of lines) {
    if (!line.trim()) continue;
    let match = line.match(/^\s*(.*?)\s*:\s*(.*?)\s*$/);
    if (match) {
      let [, name, value] = match;
      if (name && value !== undefined) {
        headers[name] = value;
      }
    }
  }
  
  return headers;
}

// Authentication Headers Builder
function buildAuthHeaders() {
  // Check for OAuth token first
  if (hasOAuthAccess()) {
    let token = getOAuthToken();
    if (!token?.accessToken) {
      return {
        headers: {},
        error: "No OAuth token available",
      };
    }
    return {
      headers: {
        Authorization: `Bearer ${token.accessToken}`,
        "anthropic-beta": ANTHROPIC_CONFIG.OAUTH_BETA_HEADER,
      },
    };
  }
  
  // Fall back to API key
  let apiKey = getAnthropicApiKey(false);
  if (!apiKey) {
    return {
      headers: {},
      error: "No API key available",
    };
  }
  
  return {
    headers: {
      "x-api-key": apiKey,
    },
  };
}

// OAuth Profile Fetching
/**
 * Fetch OAuth profile to get organization details
 * From qvA in test-fixed.js
 */
async function fetchOAuthProfile(accessToken) {
  let url = `${ANTHROPIC_CONFIG.BASE_API_URL}/api/oauth/profile`;
  
  try {
    let response = await fetch(url, {
      headers: {
        Authorization: `Bearer ${accessToken}`,
        "Content-Type": "application/json",
      },
    });
    return await response.json();
  } catch (error) {
    console.error("Failed to fetch OAuth profile:", error);
    return null;
  }
}

/**
 * Determine subscription type from OAuth token
 * From stringDecoder91 in test-fixed.js
 * Returns: "max" | "pro" | "enterprise" | "team" | null
 */
async function getSubscriptionType(accessToken) {
  let profile = await fetchOAuthProfile(accessToken);
  
  switch (profile?.organization?.organization_type) {
    case "claude_max":
      return "max";
    case "claude_pro":
      return "pro";
    case "claude_enterprise":
      return "enterprise";
    case "claude_team":
      return "team";
    default:
      return null;
  }
}

/**
 * Determine which OAuth endpoint to use based on existing token
 * If user has an existing OAuth token, fetch their subscription type
 * to determine whether to use claude.ai or console.anthropic.com
 */
async function determineOAuthEndpoint(existingToken) {
  if (!existingToken) {
    // No existing token, default to console.anthropic.com
    return false; // loginWithClaudeAi = false
  }
  
  let subscriptionType = await getSubscriptionType(existingToken);
  
  // Use claude.ai for Max and Pro subscriptions
  // Use console.anthropic.com for Enterprise, Team, or unknown
  return subscriptionType === "max" || subscriptionType === "pro";
}

// Anthropic Client Class
class AnthropicClient {
  constructor({
    baseURL = getEnvVariable("ANTHROPIC_BASE_URL"),
    apiKey = getEnvVariable("ANTHROPIC_API_KEY") ?? null,
    authToken = getEnvVariable("ANTHROPIC_AUTH_TOKEN") ?? null,
    dangerouslyAllowBrowser = false,
    timeout = 60000,
    maxRetries = 2,
    ...options
  } = {}) {
    // Check for browser environment
    if (!dangerouslyAllowBrowser && typeof window !== 'undefined') {
      throw new Error(
        "It looks like you're running in a browser-like environment.\n\n" +
        "This is disabled by default, as it risks exposing your secret API credentials to attackers.\n" +
        "If you understand the risks and have appropriate mitigations in place,\n" +
        "you can set the `dangerouslyAllowBrowser` option to `true`."
      );
    }
    
    this.baseURL = baseURL || ANTHROPIC_CONFIG.BASE_API_URL;
    this.apiKey = apiKey;
    this.authToken = authToken;
    this.timeout = timeout;
    this.maxRetries = maxRetries;
    this.logger = options.logger ?? console;
    this.logLevel = options.logLevel ?? "warn";
    this.fetchOptions = options.fetchOptions;
  }
}

// =============================================================================
// AWS AUTHENTICATION
// =============================================================================

// Cryptographic Algorithm Constants
const ChecksumAlgorithm = {
  MD5: "md5",
  CRC32: "crc32",
  CRC32C: "crc32c",
  SHA1: "sha1",
  SHA256: "sha256"
};

// AWS Environment Variables
const AWS_ACCESS_KEY_ID = "AWS_ACCESS_KEY_ID";
const AWS_SECRET_ACCESS_KEY = "AWS_SECRET_ACCESS_KEY";
const AWS_SESSION_TOKEN = "AWS_SESSION_TOKEN";
const AWS_CREDENTIAL_EXPIRATION = "AWS_CREDENTIAL_EXPIRATION";
const AWS_CREDENTIAL_SCOPE = "AWS_CREDENTIAL_SCOPE";
const AWS_ACCOUNT_ID = "AWS_ACCOUNT_ID";
const AWS_CONTAINER_CREDENTIALS_FULL_URI = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
const AWS_CONTAINER_CREDENTIALS_RELATIVE_URI = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const AWS_CONTAINER_AUTHORIZATION_TOKEN = "AWS_CONTAINER_AUTHORIZATION_TOKEN";
const AWS_EC2_METADATA_DISABLED = "AWS_EC2_METADATA_DISABLED";
const AWS_PROFILE = "AWS_PROFILE";
const AWS_REGION = "AWS_REGION";

// AWS Credential Provider: From Environment Variables
async function fromEnv(config) {
  config?.logger?.debug("@aws-sdk/credential-provider-env - fromEnv");

  const accessKeyId = process.env[AWS_ACCESS_KEY_ID];
  const secretAccessKey = process.env[AWS_SECRET_ACCESS_KEY];
  const sessionToken = process.env[AWS_SESSION_TOKEN];
  const expiration = process.env[AWS_CREDENTIAL_EXPIRATION];
  const credentialScope = process.env[AWS_CREDENTIAL_SCOPE];
  const accountId = process.env[AWS_ACCOUNT_ID];

  if (accessKeyId && secretAccessKey) {
    const credentials = {
      accessKeyId,
      secretAccessKey,
      ...(sessionToken && { sessionToken }),
      ...(expiration && { expiration: new Date(expiration) }),
      ...(credentialScope && { credentialScope }),
      ...(accountId && { accountId })
    };

    // Set credential feature flag
    setCredentialFeature(credentials, "CREDENTIALS_ENV_VARS", "g");
    return credentials;
  }

  throw new CredentialsProviderError(
    "Unable to find environment variable credentials.",
    { logger: config?.logger }
  );
}

// AWS Credential Provider: From Container Metadata (ECS)
async function fromContainerMetadata(config = {}) {
  const { timeout = 1000, maxRetries = 0 } = config;

  return retry(async () => {
    const uri = await getCmdsUri({ logger: config.logger });
    const response = await requestFromEcsImds(timeout, uri);
    const credentials = JSON.parse(response);

    if (!isValidCredentials(credentials)) {
      throw new CredentialsProviderError(
        "Invalid response received from instance metadata service.",
        { logger: config.logger }
      );
    }

    return parseCredentials(credentials);
  }, maxRetries);
}

// Get Container Metadata Service URI
async function getCmdsUri({ logger }) {
  if (process.env[AWS_CONTAINER_CREDENTIALS_RELATIVE_URI]) {
    return {
      hostname: "169.254.170.2",
      path: process.env[AWS_CONTAINER_CREDENTIALS_RELATIVE_URI]
    };
  }

  if (process.env[AWS_CONTAINER_CREDENTIALS_FULL_URI]) {
    const parsed = new URL(process.env[AWS_CONTAINER_CREDENTIALS_FULL_URI]);
    const validHosts = { localhost: true, "127.0.0.1": true };
    const validProtocols = { "http:": true, "https:": true };

    if (!validHosts[parsed.hostname]) {
      throw new CredentialsProviderError(
        `${parsed.hostname} is not a valid container metadata service hostname`,
        { tryNextLink: false, logger }
      );
    }

    if (!validProtocols[parsed.protocol]) {
      throw new CredentialsProviderError(
        `${parsed.protocol} is not a valid container metadata service protocol`,
        { tryNextLink: false, logger }
      );
    }

    return {
      ...parsed,
      port: parsed.port ? parseInt(parsed.port, 10) : undefined
    };
  }

  throw new CredentialsProviderError(
    `The container metadata credential provider cannot be used unless the ${AWS_CONTAINER_CREDENTIALS_RELATIVE_URI} or ${AWS_CONTAINER_CREDENTIALS_FULL_URI} environment variable is set`,
    { tryNextLink: false, logger }
  );
}

// AWS Credential Provider: From Instance Metadata (EC2)
async function fromInstanceMetadata(config = {}) {
  const { timeout = 1000, maxRetries = 0 } = config;
  const imdsEndpoint = await getInstanceMetadataEndpoint();

  return retry(async () => {
    const credentials = await getExtendedInstanceMetadataCredentials(
      imdsEndpoint,
      timeout
    );

    if (!isValidCredentials(credentials)) {
      throw new CredentialsProviderError(
        "Invalid response received from instance metadata service.",
        { logger: config.logger }
      );
    }

    return credentials;
  }, maxRetries);
}

// Get Instance Metadata Endpoint
async function getInstanceMetadataEndpoint() {
  return "http://169.254.169.254";
}

// AWS Credential Provider: From SSO
async function fromSSO(init = {}) {
  return async ({ callerClientConfig } = {}) => {
    init.logger?.debug("@aws-sdk/credential-provider-sso - fromSSO");

    let {
      ssoStartUrl,
      ssoAccountId,
      ssoRegion,
      ssoRoleName,
      ssoSession
    } = init;

    const { ssoClient } = init;
    const profileName = getProfileName({
      profile: init.profile ?? callerClientConfig?.profile
    });

    if (!ssoStartUrl && !ssoAccountId && !ssoRegion && !ssoRoleName && !ssoSession) {
      const profiles = await parseKnownFiles(init);
      const profile = profiles[profileName];

      if (!profile) {
        throw new CredentialsProviderError(
          `Profile ${profileName} was not found.`,
          { logger: init.logger }
        );
      }

      if (!isSsoProfile(profile)) {
        throw new CredentialsProviderError(
          `Profile ${profileName} is not configured with SSO credentials.`,
          { logger: init.logger }
        );
      }

      if (profile?.sso_session) {
        const ssoSessionData = (await loadSsoSessionData(init))[profile.sso_session];
        const conflictMsg = ` configurations in profile ${profileName} and sso-session ${profile.sso_session}`;

        if (ssoRegion && ssoRegion !== ssoSessionData.sso_region) {
          throw new CredentialsProviderError(
            "Conflicting SSO region" + conflictMsg,
            { tryNextLink: false, logger: init.logger }
          );
        }

        if (ssoStartUrl && ssoStartUrl !== ssoSessionData.sso_start_url) {
          throw new CredentialsProviderError(
            "Conflicting SSO start_url" + conflictMsg,
            { tryNextLink: false, logger: init.logger }
          );
        }

        profile.sso_region = ssoSessionData.sso_region;
        profile.sso_start_url = ssoSessionData.sso_start_url;
      }

      const {
        sso_start_url,
        sso_account_id,
        sso_region,
        sso_role_name,
        sso_session
      } = validateSsoProfile(profile, init.logger);

      return resolveSSOCredentials({
        ssoStartUrl: sso_start_url,
        ssoSession: sso_session,
        ssoAccountId: sso_account_id,
        ssoRegion: sso_region,
        ssoRoleName: sso_role_name,
        ssoClient,
        clientConfig: init.clientConfig,
        parentClientConfig: init.parentClientConfig,
        profile: profileName
      });
    } else if (!ssoStartUrl || !ssoAccountId || !ssoRegion || !ssoRoleName) {
      throw new CredentialsProviderError(
        'Incomplete configuration. The fromSSO() argument hash must include "ssoStartUrl", "ssoAccountId", "ssoRegion", "ssoRoleName"',
        { tryNextLink: false, logger: init.logger }
      );
    } else {
      return resolveSSOCredentials({
        ssoStartUrl,
        ssoSession,
        ssoAccountId,
        ssoRegion,
        ssoRoleName,
        ssoClient,
        clientConfig: init.clientConfig,
        parentClientConfig: init.parentClientConfig,
        profile: profileName
      });
    }
  };
}

// AWS Credential Provider: From Process
async function fromProcess(init = {}) {
  return async ({ callerClientConfig } = {}) => {
    init.logger?.debug("@aws-sdk/credential-provider-process - fromProcess");

    const profiles = await parseKnownFiles(init);
    return resolveProcessCredentials(
      getProfileName({
        profile: init.profile ?? callerClientConfig?.profile
      }),
      profiles,
      init.logger
    );
  };
}

// AWS Credential Provider: From Web Identity Token
async function fromWebToken(init) {
  return async (identityProperties) => {
    init.logger?.debug("@aws-sdk/credential-provider-web-identity - fromWebToken");

    const {
      roleArn,
      roleSessionName,
      webIdentityToken,
      providerId,
      policyArns,
      policy,
      durationSeconds
    } = init;

    let { roleAssumerWithWebIdentity } = init;

    if (!roleAssumerWithWebIdentity) {
      const { getDefaultRoleAssumerWithWebIdentity } = await import('./sts-client');
      roleAssumerWithWebIdentity = getDefaultRoleAssumerWithWebIdentity(
        {
          ...init.clientConfig,
          credentialProviderLogger: init.logger,
          parentClientConfig: {
            ...identityProperties?.callerClientConfig,
            ...init.parentClientConfig
          }
        },
        init.clientPlugins
      );
    }

    return roleAssumerWithWebIdentity({
      RoleArn: roleArn,
      RoleSessionName: roleSessionName ?? `aws-sdk-js-session-${Date.now()}`,
      WebIdentityToken: webIdentityToken,
      ProviderId: providerId,
      PolicyArns: policyArns,
      Policy: policy,
      DurationSeconds: durationSeconds
    });
  };
}

// AWS Credential Provider: From Token File
async function fromTokenFile(init = {}) {
  return async () => {
    init.logger?.debug("@aws-sdk/credential-provider-web-identity - fromTokenFile");

    const webIdentityTokenFile = init?.webIdentityTokenFile ?? process.env.AWS_WEB_IDENTITY_TOKEN_FILE;
    const roleArn = init?.roleArn ?? process.env.AWS_ROLE_ARN;
    const roleSessionName = init?.roleSessionName ?? process.env.AWS_ROLE_SESSION_NAME;

    if (!webIdentityTokenFile || !roleArn) {
      throw new CredentialsProviderError(
        "Web identity configuration not specified",
        { logger: init.logger }
      );
    }

    const fs = require('fs');
    const credentials = await fromWebToken({
      ...init,
      webIdentityToken: fs.readFileSync(webIdentityTokenFile, { encoding: "ascii" }),
      roleArn,
      roleSessionName
    })();

    if (webIdentityTokenFile === process.env.AWS_WEB_IDENTITY_TOKEN_FILE) {
      setCredentialFeature(credentials, "CREDENTIALS_ENV_VARS_STS_WEB_ID_TOKEN", "h");
    }

    return credentials;
  };
}

// AWS Credential Provider: From INI files
async function fromIni(init = {}) {
  return async ({ callerClientConfig } = {}) => {
    init.logger?.debug("@aws-sdk/credential-provider-ini - fromIni");

    const profileName = getProfileName({
      profile: init.profile ?? callerClientConfig?.profile
    });

    const profiles = await parseKnownFiles(init);
    const profile = profiles[profileName];

    if (!profile) {
      throw new CredentialsProviderError(
        `Profile ${profileName} could not be found or parsed in shared credentials file.`,
        { logger: init.logger }
      );
    }

    // Check for different credential sources in order
    if (profile.credential_source) {
      return resolveCredentialSource(profile.credential_source, profileName, init);
    }

    if (profile.aws_access_key_id) {
      return resolveStaticCredentials(profile, init.logger);
    }

    if (profile.credential_process) {
      return resolveProcessCredentials(profileName, profiles, init.logger);
    }

    if (profile.sso_start_url) {
      return fromSSO(init)();
    }

    if (profile.source_profile) {
      return resolveAssumeRoleCredentials(profile, profiles, init);
    }

    throw new CredentialsProviderError(
      `Profile ${profileName} did not contain credential information.`,
      { logger: init.logger }
    );
  };
}

// =============================================================================
// AWS HELPER FUNCTIONS
// =============================================================================

// Normalize credential provider to always return a function that returns a promise
function normalizeProvider(provider) {
  if (typeof provider === "function") return provider;
  const promise = Promise.resolve(provider);
  return () => promise;
}

// Helper function for HMAC signing
function hmac(sha256Constructor, key, data) {
  const hash = new sha256Constructor(key);
  hash.update(toUint8Array(data));
  return hash.digest();
}

// Convert to Uint8Array
function toUint8Array(data) {
  if (data instanceof Uint8Array) return data;
  if (typeof data === 'string') {
    const encoder = new TextEncoder();
    return encoder.encode(data);
  }
  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  throw new Error('toUint8Array only accepts string | Uint8Array | ArrayBuffer | ArrayBufferView');
}

// Convert to hex string
function toHex(data) {
  const hexChars = '0123456789abcdef';
  let result = '';
  const uint8 = data instanceof Uint8Array ? data : new Uint8Array(data);
  for (let i = 0; i < uint8.byteLength; i++) {
    const byte = uint8[i];
    result += hexChars[(byte >> 4) & 0xf];
    result += hexChars[byte & 0xf];
  }
  return result;
}

// URI encoding functions (matching AWS requirements)
function escapeUri(str) {
  // This matches the JavaScript implementation that uses encodeURIComponent
  // but then replaces back certain characters
  return encodeURIComponent(str).replace(/[!'()*]/g, (c) =>
    '%' + c.charCodeAt(0).toString(16).toUpperCase()
  );
}

function escapeUriPath(path) {
  return path.split('/').map(escapeUri).join('/');
}

// AWS SigV4 Signing Implementation
class SignatureV4 {
  constructor({ credentials, region, service, sha256, uriEscapePath = true }) {
    this.credentialProvider = normalizeProvider(credentials);
    this.region = region;
    this.service = service;
    this.sha256 = sha256;
    this.uriEscapePath = uriEscapePath;
  }

  async sign(request, options = {}) {
    const {
      signingDate = new Date(),
      signingRegion,
      signingService
    } = options;

    const credentials = await this.credentialProvider();
    this.validateResolvedCredentials(credentials);

    const region = signingRegion ?? (await this.regionProvider());
    const { longDate, shortDate } = this.formatDate(signingDate);

    const scope = `${shortDate}/${region}/${signingService ?? this.service}/aws4_request`;
    const canonicalHeaders = this.getCanonicalHeaders(request);
    const signedHeaders = this.getCanonicalHeaderList(canonicalHeaders);

    const canonicalRequest = this.createCanonicalRequest(
      request,
      canonicalHeaders,
      await this.getPayloadHash(request)
    );

    const stringToSign = await this.createStringToSign(
      longDate,
      scope,
      canonicalRequest,
      "AWS4-HMAC-SHA256"
    );

    const signature = await this.getSignature(
      longDate,
      scope,
      this.getSigningKey(credentials, region, shortDate, signingService),
      stringToSign
    );

    request.headers["Authorization"] = `AWS4-HMAC-SHA256 Credential=${credentials.accessKeyId}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`;

    if (credentials.sessionToken) {
      request.headers["x-amz-security-token"] = credentials.sessionToken;
    }

    return request;
  }

  createCanonicalRequest(request, headers, payloadHash) {
    const headerKeys = Object.keys(headers).sort();
    return `${request.method}
${this.getCanonicalPath(request)}
${this.getCanonicalQuery(request)}
${headerKeys.map(key => `${key}:${headers[key]}`).join('\n')}

${headerKeys.join(';')}
${payloadHash}`;
  }

  async createStringToSign(datetime, scope, canonicalRequest, algorithm) {
    const hasher = new this.sha256();
    hasher.update(toUint8Array(canonicalRequest));
    const requestHash = await hasher.digest();

    return `${algorithm}
${datetime}
${scope}
${toHex(requestHash)}`;
  }

  async getSigningKey(credentials, region, shortDate, service) {
    // AWS SigV4 signing key derivation using sha256 constructor
    // In the real implementation this is cached
    const kSecret = `AWS4${credentials.secretAccessKey}`;
    let signingKey = kSecret;

    // Chain of HMAC operations per AWS SigV4 spec
    for (const data of [shortDate, region, service ?? this.service, "aws4_request"]) {
      const hash = new this.sha256(signingKey);
      hash.update(toUint8Array(data));
      signingKey = await hash.digest();
    }

    return signingKey;
  }

  async getSignature(datetime, scope, signingKey, stringToSign) {
    const hash = new this.sha256(signingKey);
    hash.update(toUint8Array(stringToSign));
    const signature = await hash.digest();
    return toHex(signature);
  }

  getCanonicalPath({ path }) {
    if (this.uriEscapePath) {
      const segments = [];
      for (const segment of path.split('/')) {
        if (segment?.length === 0) continue;
        if (segment === '.') continue;
        if (segment === '..') segments.pop();
        else segments.push(segment);
      }
      const normalizedPath = `${path?.startsWith('/') ? '/' : ''}${segments.join('/')}${segments.length > 0 && path?.endsWith('/') ? '/' : ''}`;
      return escapeUri(normalizedPath).replace(/%2F/g, '/');
    }
    return path;
  }

  validateResolvedCredentials(credentials) {
    if (
      typeof credentials !== 'object' ||
      typeof credentials.accessKeyId !== 'string' ||
      typeof credentials.secretAccessKey !== 'string'
    ) {
      throw new Error('Resolved credential object is not valid');
    }
  }

  formatDate(date) {
    const longDate = date.toISOString().replace(/[\-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
    return {
      longDate,
      shortDate: longDate.slice(0, 8)
    };
  }

  getCanonicalHeaderList(headers) {
    return Object.keys(headers).sort().join(';');
  }
}

// =============================================================================
// AWS ENVIRONMENT CREDENTIAL PROVIDER
// =============================================================================

// Environment variable names
const AWS_ACCESS_KEY_ID = "AWS_ACCESS_KEY_ID";
const AWS_SECRET_ACCESS_KEY = "AWS_SECRET_ACCESS_KEY";
const AWS_SESSION_TOKEN = "AWS_SESSION_TOKEN";
const AWS_CREDENTIAL_EXPIRATION = "AWS_CREDENTIAL_EXPIRATION";
const AWS_CREDENTIAL_SCOPE = "AWS_CREDENTIAL_SCOPE";
const AWS_ACCOUNT_ID = "AWS_ACCOUNT_ID";
const AWS_PROFILE = "AWS_PROFILE";
const AWS_CONFIG_FILE = "AWS_CONFIG_FILE";
const AWS_SHARED_CREDENTIALS_FILE = "AWS_SHARED_CREDENTIALS_FILE";
const AWS_REGION = "AWS_REGION";
const AWS_DEFAULT_REGION = "AWS_DEFAULT_REGION";

// From environment variables
async function fromEnv(config = {}) {
  config?.logger?.debug("@aws-sdk/credential-provider-env - fromEnv");

  const accessKeyId = process.env[AWS_ACCESS_KEY_ID];
  const secretAccessKey = process.env[AWS_SECRET_ACCESS_KEY];
  const sessionToken = process.env[AWS_SESSION_TOKEN];
  const expiration = process.env[AWS_CREDENTIAL_EXPIRATION];
  const credentialScope = process.env[AWS_CREDENTIAL_SCOPE];
  const accountId = process.env[AWS_ACCOUNT_ID];

  if (accessKeyId && secretAccessKey) {
    const credentials = {
      accessKeyId,
      secretAccessKey,
      ...(sessionToken && { sessionToken }),
      ...(expiration && { expiration: new Date(expiration) }),
      ...(credentialScope && { credentialScope }),
      ...(accountId && { accountId }),
    };
    // In real implementation, sets credential feature flag
    return credentials;
  }

  throw new Error("Unable to find environment variable credentials.");
}

// =============================================================================
// AWS CONTAINER METADATA CREDENTIAL PROVIDER
// =============================================================================

const AWS_CONTAINER_CREDENTIALS_FULL_URI = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
const AWS_CONTAINER_CREDENTIALS_RELATIVE_URI = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const AWS_CONTAINER_AUTHORIZATION_TOKEN = "AWS_CONTAINER_AUTHORIZATION_TOKEN";

async function fromContainerMetadata(init = {}) {
  const { timeout = 1000, maxRetries = 0 } = init;

  return retry(async () => {
    const uri = await getCmdsUri({ logger: init.logger });
    const response = await requestFromEcsImds(timeout, uri);
    const credentials = JSON.parse(response);

    if (!isValidCredentials(credentials)) {
      throw new Error("Invalid response received from container metadata service.");
    }

    return formatCredentials(credentials);
  }, maxRetries);
}

async function getCmdsUri({ logger }) {
  if (process.env[AWS_CONTAINER_CREDENTIALS_RELATIVE_URI]) {
    return {
      hostname: "169.254.170.2",
      path: process.env[AWS_CONTAINER_CREDENTIALS_RELATIVE_URI],
    };
  }

  if (process.env[AWS_CONTAINER_CREDENTIALS_FULL_URI]) {
    const url = new URL(process.env[AWS_CONTAINER_CREDENTIALS_FULL_URI]);
    const validHosts = { localhost: true, "127.0.0.1": true };
    const validProtocols = { "http:": true, "https:": true };

    if (!validHosts[url.hostname]) {
      throw new Error(`${url.hostname} is not a valid container metadata service hostname`);
    }

    if (!validProtocols[url.protocol]) {
      throw new Error(`${url.protocol} is not a valid container metadata service protocol`);
    }

    return {
      protocol: url.protocol,
      hostname: url.hostname,
      port: url.port ? parseInt(url.port, 10) : undefined,
      path: url.pathname,
    };
  }

  throw new Error(
    `The container metadata credential provider cannot be used unless the ${AWS_CONTAINER_CREDENTIALS_RELATIVE_URI} or ${AWS_CONTAINER_CREDENTIALS_FULL_URI} environment variable is set`
  );
}

async function requestFromEcsImds(timeout, options) {
  const headers = { ...options.headers };

  if (process.env[AWS_CONTAINER_AUTHORIZATION_TOKEN]) {
    headers.Authorization = process.env[AWS_CONTAINER_AUTHORIZATION_TOKEN];
  }

  // In real implementation, makes HTTP request
  // This is simplified
  const response = await httpRequest({
    ...options,
    headers,
    timeout,
  });

  return response.toString();
}

// =============================================================================
// AWS INSTANCE METADATA CREDENTIAL PROVIDER (EC2)
// =============================================================================

const AWS_EC2_METADATA_V1_DISABLED = "AWS_EC2_METADATA_V1_DISABLED";
const AWS_EC2_METADATA_SERVICE_ENDPOINT = "AWS_EC2_METADATA_SERVICE_ENDPOINT";
const AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE = "AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE";
const AWS_EC2_METADATA_DISABLED = "AWS_EC2_METADATA_DISABLED";

async function fromInstanceMetadata(init = {}) {
  const disableV1 = init.ec2MetadataV1Disabled ||
                    process.env[AWS_EC2_METADATA_V1_DISABLED] === "true";

  if (process.env[AWS_EC2_METADATA_DISABLED] === "true") {
    throw new Error("EC2 Instance Metadata Service is disabled");
  }

  const { timeout = 1000, maxRetries = 0 } = init;
  const endpoint = process.env[AWS_EC2_METADATA_SERVICE_ENDPOINT] || "http://169.254.169.254";

  return retry(async () => {
    let token;

    // Try to get IMDSv2 token
    if (!disableV1) {
      try {
        token = await getMetadataToken(endpoint, timeout);
      } catch (error) {
        if (error.statusCode === 403 || error.statusCode === 404 || error.statusCode === 405) {
          // Fall back to IMDSv1
          init.logger?.debug("Falling back to IMDSv1");
        } else {
          throw error;
        }
      }
    }

    // Get credentials from instance metadata
    const credentialsPath = "/latest/meta-data/iam/security-credentials/";
    const headers = token ? { "x-aws-ec2-metadata-token": token } : {};

    // Get role name
    const roleResponse = await httpRequest({
      hostname: new URL(endpoint).hostname,
      path: credentialsPath,
      headers,
      timeout,
    });

    const roleName = roleResponse.toString().trim();

    // Get credentials for role
    const credsResponse = await httpRequest({
      hostname: new URL(endpoint).hostname,
      path: `${credentialsPath}${roleName}`,
      headers,
      timeout,
    });

    const credentials = JSON.parse(credsResponse.toString());

    return formatCredentials(credentials);
  }, maxRetries);
}

async function getMetadataToken(endpoint, timeout) {
  const response = await httpRequest({
    hostname: new URL(endpoint).hostname,
    path: "/latest/api/token",
    method: "PUT",
    headers: {
      "x-aws-ec2-metadata-token-ttl-seconds": "21600",
    },
    timeout,
  });

  return response.toString();
}

// =============================================================================
// AWS UTILITY FUNCTIONS
// =============================================================================

function isValidCredentials(credentials) {
  return credentials &&
         typeof credentials.AccessKeyId === "string" &&
         typeof credentials.SecretAccessKey === "string";
}

function formatCredentials(credentials) {
  return {
    accessKeyId: credentials.AccessKeyId || credentials.accessKeyId,
    secretAccessKey: credentials.SecretAccessKey || credentials.secretAccessKey,
    sessionToken: credentials.SessionToken || credentials.sessionToken || credentials.Token,
    expiration: credentials.Expiration ? new Date(credentials.Expiration) : undefined,
    accountId: credentials.AccountId,
  };
}

async function retry(fn, maxRetries) {
  let lastError;
  for (let i = 0; i <= maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error;
      if (i === maxRetries) throw error;
      // Exponential backoff
      await new Promise(resolve => setTimeout(resolve, Math.pow(2, i) * 100));
    }
  }
  throw lastError;
}

// HTTP request function using Node.js http/https modules
async function httpRequest(options) {
  const http = require('http');
  const https = require('https');

  const isHttps = options.protocol === 'https:' ||
                  (options.hostname && options.port === 443);
  const httpModule = isHttps ? https : http;

  return new Promise((resolve, reject) => {
    const req = httpModule.request(options, (res) => {
      const chunks = [];

      res.on('data', (chunk) => {
        chunks.push(chunk);
      });

      res.on('end', () => {
        const buffer = Buffer.concat(chunks);
        // Add status code for error handling
        buffer.statusCode = res.statusCode;
        buffer.headers = res.headers;

        if (res.statusCode >= 400) {
          const error = new Error(`HTTP ${res.statusCode}`);
          error.statusCode = res.statusCode;
          error.response = buffer.toString();
          reject(error);
        } else {
          resolve(buffer);
        }
      });
    });

    req.on('error', reject);

    if (options.timeout) {
      req.setTimeout(options.timeout, () => {
        req.destroy();
        const error = new Error('TimeoutError');
        error.message = 'TimeoutError';
        reject(error);
      });
    }

    if (options.body) {
      req.write(options.body);
    }

    req.end();
  });
}

// =============================================================================
// AWS STS ROLE ASSUMPTION
// =============================================================================

// Get account ID from assumed role ARN
function getAccountIdFromAssumedRoleUser(assumedRoleUser) {
  if (typeof assumedRoleUser?.Arn === "string") {
    const parts = assumedRoleUser.Arn.split(":");
    if (parts.length > 4 && parts[4] !== "") {
      return parts[4];
    }
  }
  return undefined;
}

// Resolve AWS region for STS client
async function resolveRegion(regionProvider, parentRegion, logger) {
  const providerRegion = typeof regionProvider === "function"
    ? await regionProvider()
    : regionProvider;
  const parentClientRegion = typeof parentRegion === "function"
    ? await parentRegion()
    : parentRegion;

  const defaultRegion = "us-east-1";

  logger?.debug?.(
    "@aws-sdk/client-sts::resolveRegion",
    "accepting first of:",
    `${providerRegion} (provider)`,
    `${parentClientRegion} (parent client)`,
    `${defaultRegion} (STS default)`
  );

  return providerRegion ?? parentClientRegion ?? defaultRegion;
}

// Default role assumer for AWS STS AssumeRole
function getDefaultRoleAssumer(stsOptions, STSClient) {
  let stsClient;

  return async (credentials, params) => {
    if (!stsClient) {
      const {
        logger = stsOptions?.parentClientConfig?.logger,
        region,
        requestHandler = stsOptions?.parentClientConfig?.requestHandler,
        credentialProviderLogger
      } = stsOptions;

      const resolvedRegion = await resolveRegion(
        region,
        stsOptions?.parentClientConfig?.region,
        credentialProviderLogger
      );

      stsClient = new STSClient({
        profile: stsOptions?.parentClientConfig?.profile,
        credentialDefaultProvider: () => async () => credentials,
        region: resolvedRegion,
        requestHandler: requestHandler,
        logger: logger,
      });
    }

    const { Credentials, AssumedRoleUser } = await stsClient.send(
      new AssumeRoleCommand(params)
    );

    if (!Credentials || !Credentials.AccessKeyId || !Credentials.SecretAccessKey) {
      throw new Error(
        `Invalid response from STS.assumeRole call with role ${params.RoleArn}`
      );
    }

    const accountId = getAccountIdFromAssumedRoleUser(AssumedRoleUser);

    const assumedCredentials = {
      accessKeyId: Credentials.AccessKeyId,
      secretAccessKey: Credentials.SecretAccessKey,
      sessionToken: Credentials.SessionToken,
      expiration: Credentials.Expiration,
      ...(Credentials.CredentialScope && {
        credentialScope: Credentials.CredentialScope
      }),
      ...(accountId && { accountId }),
    };

    // Set credential feature flag
    // setCredentialFeature(assumedCredentials, "CREDENTIALS_STS_ASSUME_ROLE", "i");

    return assumedCredentials;
  };
}

// Default role assumer for AWS STS AssumeRoleWithWebIdentity
function getDefaultRoleAssumerWithWebIdentity(stsOptions, STSClient) {
  let stsClient;

  return async (params) => {
    if (!stsClient) {
      const {
        logger = stsOptions?.parentClientConfig?.logger,
        region,
        requestHandler = stsOptions?.parentClientConfig?.requestHandler,
        credentialProviderLogger
      } = stsOptions;

      const resolvedRegion = await resolveRegion(
        region,
        stsOptions?.parentClientConfig?.region,
        credentialProviderLogger
      );

      stsClient = new STSClient({
        profile: stsOptions?.parentClientConfig?.profile,
        region: resolvedRegion,
        requestHandler: requestHandler,
        logger: logger,
      });
    }

    const { Credentials, AssumedRoleUser } = await stsClient.send(
      new AssumeRoleWithWebIdentityCommand(params)
    );

    if (!Credentials || !Credentials.AccessKeyId || !Credentials.SecretAccessKey) {
      throw new Error(
        `Invalid response from STS.assumeRoleWithWebIdentity call with role ${params.RoleArn}`
      );
    }

    const accountId = getAccountIdFromAssumedRoleUser(AssumedRoleUser);

    const assumedCredentials = {
      accessKeyId: Credentials.AccessKeyId,
      secretAccessKey: Credentials.SecretAccessKey,
      sessionToken: Credentials.SessionToken,
      expiration: Credentials.Expiration,
      ...(Credentials.CredentialScope && {
        credentialScope: Credentials.CredentialScope
      }),
      ...(accountId && { accountId }),
    };

    // Set credential feature flags
    // if (accountId) setCredentialFeature(assumedCredentials, "RESOLVED_ACCOUNT_ID", "T");
    // setCredentialFeature(assumedCredentials, "CREDENTIALS_STS_ASSUME_ROLE_WEB_ID", "k");

    return assumedCredentials;
  };
}

// Placeholder for actual STS commands (these would come from AWS SDK)
class AssumeRoleCommand {
  constructor(params) {
    this.input = params;
  }
}

class AssumeRoleWithWebIdentityCommand {
  constructor(params) {
    this.input = params;
  }
}

// =============================================================================
// AWS SSO CREDENTIAL PROVIDER
// =============================================================================

// Get SSO token file path
function getSSOTokenFilepath(ssoStartUrl) {
  const crypto = require('crypto');
  const os = require('os');
  const path = require('path');

  const hasher = crypto.createHash('sha1');
  hasher.update(ssoStartUrl);
  const sessionName = hasher.digest('hex');

  return path.join(
    os.homedir(),
    '.aws',
    'sso',
    'cache',
    `${sessionName}.json`
  );
}

// Read SSO token from cache file
async function getSSOTokenFromFile(ssoStartUrl) {
  const fs = require('fs').promises;
  const filepath = getSSOTokenFilepath(ssoStartUrl);
  const content = await fs.readFile(filepath, 'utf8');
  return JSON.parse(content);
}

// Write SSO token to cache file
async function writeSSOTokenToFile(ssoStartUrl, token) {
  const fs = require('fs').promises;
  const path = require('path');
  const filepath = getSSOTokenFilepath(ssoStartUrl);

  // Ensure directory exists
  const dir = path.dirname(filepath);
  await fs.mkdir(dir, { recursive: true });

  await fs.writeFile(filepath, JSON.stringify(token, null, 2), 'utf8');
}

async function fromSSO(init = {}) {
  const {
    ssoStartUrl,
    ssoAccountId,
    ssoRegion,
    ssoRoleName,
    ssoSession,
    ssoClient,
    clientConfig,
    parentClientConfig,
    profile,
    logger
  } = init;

  // Validate required parameters
  if (!ssoStartUrl || !ssoAccountId || !ssoRegion || !ssoRoleName) {
    throw new Error(
      'Incomplete configuration. The fromSSO() argument hash must include "ssoStartUrl", "ssoAccountId", "ssoRegion", "ssoRoleName"'
    );
  }

  logger?.debug("@aws-sdk/credential-provider-sso - fromSSO");

  let ssoToken;

  // Handle SSO session vs direct SSO
  if (ssoSession) {
    // Load token from SSO session (AWS SSO v2)
    try {
      // In real implementation, would use fromSso token provider
      // to handle OAuth refresh flow
      const tokenProvider = await fromSsoTokenProvider({ profile });
      const token = await tokenProvider();
      ssoToken = {
        accessToken: token.token,
        expiresAt: new Date(token.expiration).toISOString()
      };
    } catch (error) {
      throw new Error(
        `The SSO session associated with this profile is invalid: ${error.message}`
      );
    }
  } else {
    // Load token from cache file (AWS SSO v1)
    try {
      ssoToken = await getSSOTokenFromFile(ssoStartUrl);
    } catch (error) {
      throw new Error(
        "The SSO session associated with this profile is invalid. " +
        "To refresh this SSO session run aws sso login with the corresponding profile."
      );
    }
  }

  // Check if token is expired
  if (new Date(ssoToken.expiresAt).getTime() - Date.now() <= 0) {
    throw new Error(
      "The SSO session associated with this profile has expired. " +
      "To refresh this SSO session run aws sso login with the corresponding profile."
    );
  }

  const { accessToken } = ssoToken;

  // Create SSO client if not provided
  const client = ssoClient || new SSOClient({
    ...clientConfig,
    logger: clientConfig?.logger ?? parentClientConfig?.logger,
    region: clientConfig?.region ?? ssoRegion
  });

  // Get role credentials from SSO
  let result;
  try {
    result = await client.send(new GetRoleCredentialsCommand({
      accountId: ssoAccountId,
      roleName: ssoRoleName,
      accessToken: accessToken
    }));
  } catch (error) {
    throw new Error(`Failed to get SSO role credentials: ${error.message}`);
  }

  const { roleCredentials } = result;

  if (!roleCredentials?.accessKeyId || !roleCredentials?.secretAccessKey) {
    throw new Error("SSO returned incomplete credentials");
  }

  const credentials = {
    accessKeyId: roleCredentials.accessKeyId,
    secretAccessKey: roleCredentials.secretAccessKey,
    sessionToken: roleCredentials.sessionToken,
    expiration: roleCredentials.expiration
      ? new Date(roleCredentials.expiration)
      : undefined
  };

  // Set credential feature flag
  // setCredentialFeature(credentials, "CREDENTIALS_SSO", "s");

  return credentials;
}

// SSO Client implementation using the AWS infrastructure we already extracted
class SSOClient {
  constructor(config) {
    this.config = config;
    this.region = config.region || 'us-east-1';
    this.endpoint = `https://portal.sso.${this.region}.amazonaws.com`;
  }

  async send(command) {
    // SSO API uses bearer token auth, not SigV4
    // This is why it's different from STS

    if (command instanceof GetRoleCredentialsCommand) {
      const { accountId, roleName, accessToken } = command.input;

      const response = await httpRequest({
        hostname: `portal.sso.${this.region}.amazonaws.com`,
        path: `/federation/credentials`,
        method: 'GET',
        headers: {
          'x-amz-sso_bearer_token': accessToken,
          'Content-Type': 'application/json',
        },
        query: {
          account_id: accountId,
          role_name: roleName,
        }
      });

      return JSON.parse(response.toString());
    }

    throw new Error(`Unknown command: ${command.constructor.name}`);
  }
}

class GetRoleCredentialsCommand {
  constructor(params) {
    this.input = params;
  }
}

// =============================================================================
// AWS WEB IDENTITY TOKEN FILE CREDENTIAL PROVIDER
// =============================================================================

const AWS_WEB_IDENTITY_TOKEN_FILE = "AWS_WEB_IDENTITY_TOKEN_FILE";
const AWS_ROLE_ARN = "AWS_ROLE_ARN";
const AWS_ROLE_SESSION_NAME = "AWS_ROLE_SESSION_NAME";

async function fromWebToken(init = {}) {
  init.logger?.debug("@aws-sdk/credential-provider-web-identity - fromWebToken");

  const tokenFile = process.env[AWS_WEB_IDENTITY_TOKEN_FILE];
  const roleArn = process.env[AWS_ROLE_ARN];
  const roleSessionName = process.env[AWS_ROLE_SESSION_NAME] || "web-identity-session";

  if (!tokenFile || !roleArn) {
    throw new Error(
      "Web identity credentials not configured. " +
      "Set AWS_WEB_IDENTITY_TOKEN_FILE and AWS_ROLE_ARN environment variables."
    );
  }

  // Read the web identity token from file
  const fs = require('fs').promises;
  const webIdentityToken = await fs.readFile(tokenFile, 'utf8');

  // Use STS AssumeRoleWithWebIdentity
  const roleAssumer = getDefaultRoleAssumerWithWebIdentity(init);

  return roleAssumer({
    RoleArn: roleArn,
    RoleSessionName: roleSessionName,
    WebIdentityToken: webIdentityToken.trim(),
  });
}

// =============================================================================
// AWS COGNITO IDENTITY CREDENTIAL PROVIDER
// =============================================================================

async function fromCognitoIdentity(params) {
  const {
    identityId,
    logins,
    customRoleArn,
    credentialProvider,
    identityPoolId
  } = params;

  // This would use AWS Cognito Identity service to get credentials
  // Implementation requires Cognito Identity client

  throw new Error("Cognito Identity credential provider requires full AWS SDK implementation");
}

// =============================================================================
// AWS CREDENTIAL PROVIDER UTILITIES
// =============================================================================

// Chain multiple credential providers
function chain(...providers) {
  return async () => {
    let lastError;
    for (const provider of providers) {
      try {
        const credentials = await provider();
        if (credentials) return credentials;
      } catch (error) {
        lastError = error;
        // Continue to next provider
      }
    }
    throw lastError || new Error("Could not load credentials from any providers");
  };
}

// Memoize credentials with expiration
function memoize(provider, isExpired, requiresRefresh) {
  let cached;
  let hasResult;
  let isConstant = false;

  return async () => {
    if (!hasResult || (requiresRefresh && requiresRefresh(cached))) {
      cached = await provider();
      hasResult = true;
    }

    if (isExpired && isExpired(cached)) {
      cached = await provider();
      hasResult = true;
    }

    return cached;
  };
}

// AWS Default Credential Provider Chain
async function defaultProvider(config = {}) {
  return memoize(
    chain(
      // 1. Check environment variables
      async () => {
        if (config.profile ?? process.env[AWS_PROFILE]) {
          if (process.env[AWS_ACCESS_KEY_ID] && process.env[AWS_SECRET_ACCESS_KEY]) {
            console.warn(
              "@aws-sdk/credential-provider-node - defaultProvider::fromEnv WARNING:\n" +
              "Multiple credential sources detected: Both AWS_PROFILE and AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY are set.\n" +
              "This SDK will proceed with the AWS_PROFILE value."
            );
          }
          throw new CredentialsProviderError(
            "AWS_PROFILE is set, skipping fromEnv provider.",
            { logger: config.logger, tryNextLink: true }
          );
        }
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromEnv");
        return fromEnv(config)();
      },

      // 2. Check SSO credentials
      async () => {
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromSSO");
        const { ssoStartUrl, ssoAccountId, ssoRegion, ssoRoleName, ssoSession } = config;

        if (!ssoStartUrl && !ssoAccountId && !ssoRegion && !ssoRoleName && !ssoSession) {
          throw new CredentialsProviderError(
            "Skipping SSO provider in default chain (inputs do not include SSO fields).",
            { logger: config.logger }
          );
        }

        return fromSSO(config)();
      },

      // 3. Check INI file credentials
      async () => {
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromIni");
        return fromIni(config)();
      },

      // 4. Check process credentials
      async () => {
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromProcess");
        return fromProcess(config)();
      },

      // 5. Check web identity token
      async () => {
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromWebToken");
        return fromWebToken(config)();
      },

      // 6. Check ECS container credentials
      async () => {
        if (process.env[AWS_CONTAINER_CREDENTIALS_RELATIVE_URI] ||
            process.env[AWS_CONTAINER_CREDENTIALS_FULL_URI]) {
          config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromContainerMetadata");
          return fromContainerMetadata(config)();
        }
        throw new CredentialsProviderError("Container credentials not available");
      },

      // 7. Check EC2 instance metadata
      async () => {
        if (process.env[AWS_EC2_METADATA_DISABLED] === "true") {
          throw new CredentialsProviderError(
            "EC2 Instance Metadata Service access disabled",
            { logger: config.logger }
          );
        }
        config.logger?.debug("@aws-sdk/credential-provider-node - defaultProvider::fromInstanceMetadata");
        return fromInstanceMetadata(config)();
      }
    )
  );
}

// Main AWS Auth Feature Checker
async function checkAWSAuthFeatures(context, config, options) {
  // Check for RPC v2 CBOR protocol
  if (options.request?.headers?.["smithy-protocol"] === "rpc-v2-cbor") {
    setFeature(context, "PROTOCOL_RPC_V2_CBOR", "M");
  }
  
  // Check retry strategy
  if (typeof config.retryStrategy === "function") {
    let strategy = await config.retryStrategy();
    if (typeof strategy.acquireInitialRetryToken === "function") {
      if (strategy.constructor?.name?.includes("Adaptive")) {
        setFeature(context, "RETRY_MODE_ADAPTIVE", "F");
      } else {
        setFeature(context, "RETRY_MODE_STANDARD", "E");
      }
    } else {
      setFeature(context, "RETRY_MODE_LEGACY", "D");
    }
  }
  
  // Check account ID endpoint mode
  if (typeof config.accountIdEndpointMode === "function") {
    let endpoint = context.endpointV2;
    if (String(endpoint?.url?.hostname).match(/\d{12}\.ddb/)) {
      setFeature(context, "ACCOUNT_ID_ENDPOINT", "O");
    }
    
    switch (await config.accountIdEndpointMode?.()) {
      case "disabled":
        setFeature(context, "ACCOUNT_ID_MODE_DISABLED", "Q");
        break;
      case "preferred":
        setFeature(context, "ACCOUNT_ID_MODE_PREFERRED", "P");
        break;
      case "required":
        setFeature(context, "ACCOUNT_ID_MODE_REQUIRED", "R");
        break;
    }
  }
  
  // Check for resolved identity
  let identity = context.__smithy_context?.selectedHttpAuthScheme?.identity;
  if (identity?.$source) {
    if (identity.accountId) {
      setFeature(context, "RESOLVED_ACCOUNT_ID", "T");
    }
    for (let [key, value] of Object.entries(identity.$source ?? {})) {
      setFeature(context, key, value);
    }
  }
}

// SSO Token Management
async function getSSOTokenFromFile(ssoStartUrl) {
  const fs = require('fs').promises;
  let tokenPath = getSSOTokenFilepath(ssoStartUrl);
  let content = await fs.readFile(tokenPath, "utf8");
  return JSON.parse(content);
}

// Get Resolved Signing Region
function getResolvedSigningRegion(hostname, { signingRegion, regionRegex, useFipsEndpoint }) {
  if (signingRegion) {
    return signingRegion;
  } else if (useFipsEndpoint) {
    let regex = regionRegex
      .replace("\\\\", "\\")
      .replace(/^\^/g, "\\.")
      .replace(/\$$/g, "\\.");
    let match = hostname.match(regex);
    if (match) {
      return match[0].slice(1, -1);
    }
  }
  return null;
}

// Checksum Constructors
function createChecksumConfiguration(algorithms) {
  let checksums = [];
  
  if (algorithms.sha256 !== undefined) {
    checksums.push({
      algorithmId: () => "sha256",
      checksumConstructor: () => algorithms.sha256,
    });
  }
  
  if (algorithms.md5 != null) {
    checksums.push({
      algorithmId: () => "md5",
      checksumConstructor: () => algorithms.md5,
    });
  }
  
  return {
    addChecksumAlgorithm(algorithm) {
      checksums.push(algorithm);
    },
    checksumAlgorithms() {
      return checksums;
    },
  };
}

// =============================================================================
// SESSION MANAGEMENT
// =============================================================================

// Create New Session
function makeSession(initialData) {
  let timestamp = Date.now() / 1000;
  let session = {
    sid: generateUUID(),
    init: true,
    timestamp: timestamp,
    started: timestamp,
    duration: 0,
    status: "ok",
    errors: 0,
    ignoreDuration: false,
    toJSON: () => serializeSession(session),
  };
  
  if (initialData) {
    updateSession(session, initialData);
  }
  
  return session;
}

// Update Session
function updateSession(session, updates = {}) {
  // Update user information
  if (updates.user) {
    if (!session.ipAddress && updates.user.ip_address) {
      session.ipAddress = updates.user.ip_address;
    }
    if (!session.did && !updates.did) {
      session.did = updates.user.id || updates.user.email || updates.user.username;
    }
  }
  
  // Update timestamp
  session.timestamp = updates.timestamp || Date.now() / 1000;
  
  // Update various session properties
  if (updates.abnormal_mechanism) {
    session.abnormal_mechanism = updates.abnormal_mechanism;
  }
  if (updates.ignoreDuration) {
    session.ignoreDuration = updates.ignoreDuration;
  }
  if (updates.sid) {
    session.sid = updates.sid.length === 32 ? updates.sid : generateUUID();
  }
  if (updates.init !== undefined) {
    session.init = updates.init;
  }
  if (!session.did && updates.did) {
    session.did = `${updates.did}`;
  }
  if (typeof updates.started === "number") {
    session.started = updates.started;
  }
  
  // Calculate duration
  if (session.ignoreDuration) {
    session.duration = undefined;
  } else if (typeof updates.duration === "number") {
    session.duration = updates.duration;
  } else {
    let duration = session.timestamp - session.started;
    session.duration = duration >= 0 ? duration : 0;
  }
  
  // Update environment info
  if (updates.release) session.release = updates.release;
  if (updates.environment) session.environment = updates.environment;
  if (!session.ipAddress && updates.ipAddress) {
    session.ipAddress = updates.ipAddress;
  }
  if (!session.userAgent && updates.userAgent) {
    session.userAgent = updates.userAgent;
  }
  if (typeof updates.errors === "number") {
    session.errors = updates.errors;
  }
  if (updates.status) {
    session.status = updates.status;
  }
}

// Close Session
function closeSession(session, status) {
  let updates = {};
  if (status) {
    updates = { status: status };
  } else if (session.status === "ok") {
    updates = { status: "exited" };
  }
  updateSession(session, updates);
}

// Session Storage (Class Methods)
class SessionManager {
  setSession(session) {
    if (!session) {
      delete this._session;
    } else {
      this._session = session;
    }
    this._notifyScopeListeners();
    return this;
  }
  
  getSession() {
    return this._session;
  }
}

// =============================================================================
// CLIENT MANAGEMENT
// =============================================================================

class ClientManager {
  setClient(client) {
    this._client = client;
  }
  
  getClient() {
    return this._client;
  }
}

// =============================================================================
// PROXY AUTHENTICATION
// =============================================================================

// Basic Auth for HTTP Proxy
function addProxyAuthentication(headers, proxyUrl) {
  let url = new URL(proxyUrl);
  if (url.username || url.password) {
    let credentials = `${decodeURIComponent(url.username)}:${decodeURIComponent(url.password)}`;
    headers["Proxy-Authorization"] = `Basic ${Buffer.from(credentials).toString("base64")}`;
  }
  return headers;
}

// =============================================================================
// HELPER FUNCTIONS (Stubs for missing implementations)
// =============================================================================

// These would need to be implemented based on your specific requirements
function isApiKeyApproved(apiKey) {
  // Check if API key is in approved list
  return false;
}

function getApiKeyFromHelper() {
  // Execute API key helper script/command
  return null;
}

function getManagedApiKey() {
  // Get API key from platform keychain
  return null;
}

/**
 * Get stored OAuth token from system credential store
 * From UZ in test-fixed.js
 * This reads OAuth tokens stored by Claude Desktop
 */
function getOAuthToken() {
  try {
    // XJ() returns credential storage interface (Keychain on macOS, etc.)
    let storedCredentials = getCredentialStorage().read();
    let oauthToken = storedCredentials?.claudeAiOauth;
    
    if (!oauthToken?.accessToken) return null;
    
    // Handle legacy format that used isMax flag
    if (!oauthToken.subscriptionType) {
      let subscriptionType = oauthToken.isMax === false ? "pro" : "max";
      return {
        ...oauthToken,
        subscriptionType: subscriptionType,
      };
    }
    
    return oauthToken;
  } catch (error) {
    console.error("Failed to read OAuth token:", error);
    return null;
  }
}

/**
 * Get credential storage interface based on platform
 * From XJ in test-fixed.js
 */
function getCredentialStorage() {
  if (process.platform === "darwin") {
    // macOS uses Keychain
    let keychainStorage = createKeychainStorage();
    return createFallbackStorage(keychainStorage);
  }
  // Other platforms use plaintext file storage
  return createPlaintextStorage();
}

/**
 * Get service name for keychain storage
 * From ti in test-fixed.js
 */
function getServiceName(suffix = "") {
  const crypto = require('crypto');
  let configDir = getConfigPath();
  let hash = !process.env.CLAUDE_CONFIG_DIR
    ? ""
    : `-${crypto.createHash("sha256").update(configDir).digest("hex").substring(0, 8)}`;
  // str128 is the base service name, likely "anthropic-claude-code" or similar
  const str128 = "anthropic-claude-code"; // This would be from the constants
  return `${str128}${suffix}${hash}`;
}

/**
 * Get config directory path
 * From checker64 in test-fixed.js
 */
function getConfigPath() {
  const os = require('os');
  const path = require('path');
  
  if (process.env.CLAUDE_CONFIG_DIR) return process.env.CLAUDE_CONFIG_DIR;
  if (process.env.XDG_CONFIG_HOME) {
    return path.join(process.env.XDG_CONFIG_HOME, "claude");
  }
  return path.join(os.homedir(), ".claude");
}

/**
 * Execute command and return output
 * Wrapper for execSync - from EZ in test-fixed.js
 */
function execSync(command) {
  const { execSync: exec } = require('child_process');
  try {
    return exec(command, { encoding: 'utf8' }).trim();
  } catch (error) {
    return null;
  }
}

/**
 * Create macOS Keychain storage interface
 * From NvA in test-fixed.js
 */
function createKeychainStorage() {
  let serviceName = getServiceName("-credentials");
  return {
    name: "keychain",
    read() {
      try {
        // Execute security command to read from keychain
        let result = execSync(
          `security find-generic-password -a $USER -w -s "${serviceName}"`
        );
        if (result) return JSON.parse(result);
      } catch (error) {
        return null;
      }
      return null;
    },
    update(credentials) {
      try {
        let jsonStr = JSON.stringify(credentials).replace(/"/g, '\\"');
        let cmd = `security add-generic-password -U -a $USER -s "${serviceName}" -w "${jsonStr}"`;
        execSync(cmd);
        return { success: true };
      } catch (error) {
        return { success: false };
      }
    },
    delete() {
      try {
        execSync(`security delete-generic-password -a $USER -s "${serviceName}"`);
        return true;
      } catch (error) {
        return false;
      }
    }
  };
}

/**
 * Create plaintext file storage interface
 * From func154 in test-fixed.js
 */
function createPlaintextStorage() {
  let configPath = getConfigPath();
  let credentialsFile = path.join(configPath, ".credentials.json");
  
  return {
    name: "plaintext",
    read() {
      if (fs.existsSync(credentialsFile)) {
        try {
          let content = fs.readFileSync(credentialsFile, { encoding: "utf8" });
          return JSON.parse(content);
        } catch (error) {
          return null;
        }
      }
      return null;
    },
    update(credentials) {
      try {
        if (!fs.existsSync(configPath)) {
          fs.mkdirSync(configPath);
        }
        fs.writeFileSync(credentialsFile, JSON.stringify(credentials), {
          encoding: "utf8",
          flush: false
        });
        fs.chmodSync(credentialsFile, 0o600); // 384 in decimal = 0600 in octal
        return {
          success: true,
          warning: "Warning: Storing credentials in plaintext."
        };
      } catch (error) {
        return { success: false };
      }
    },
    delete() {
      try {
        if (fs.existsSync(credentialsFile)) {
          fs.unlinkSync(credentialsFile);
          return true;
        }
      } catch (error) {
        return false;
      }
      return false;
    }
  };
}

/**
 * Create storage with plaintext fallback
 * From stringDecoder89 in test-fixed.js
 */
function createFallbackStorage(primaryStorage) {
  let fallbackStorage = createPlaintextStorage();
  
  return {
    name: `${primaryStorage.name}-with-${fallbackStorage.name}-fallback`,
    read() {
      let primaryData = primaryStorage.read();
      if (primaryData !== null && primaryData !== undefined) {
        return primaryData;
      }
      return fallbackStorage.read() || {};
    },
    update(credentials) {
      let primaryResult = primaryStorage.update(credentials);
      if (primaryResult.success) {
        fallbackStorage.delete();
        return primaryResult;
      }
      let fallbackResult = fallbackStorage.update(credentials);
      if (fallbackResult.success) {
        return {
          success: true,
          warning: fallbackResult.warning
        };
      }
      return { success: false };
    },
    delete() {
      let primaryDeleted = primaryStorage.delete();
      let fallbackDeleted = fallbackStorage.delete();
      return primaryDeleted || fallbackDeleted;
    }
  };
}

function hasValidScopes(token) {
  // Check if token has required scopes
  return token?.scopes?.includes(ANTHROPIC_CONFIG.OAUTH_SCOPE);
}

function hasOAuthAccess() {
  // Check if OAuth is available and valid
  return false;
}

function setFeature(context, feature, value) {
  // Set feature flag in context
  if (!context.features) context.features = {};
  context.features[feature] = value;
}

function getSSOTokenFilepath(ssoStartUrl) {
  // Generate SSO token file path with SHA1 hash
  const crypto = require('crypto');
  const hasher = crypto.createHash('sha1');
  hasher.update(ssoStartUrl);
  const hashedUrl = hasher.digest('hex');

  const os = require('os');
  const path = require('path');
  return path.join(os.homedir(), '.aws', 'sso', 'cache', `${hashedUrl}.json`);
}

// AWS Role Assumers
async function getDefaultRoleAssumer(stsClientConfig, stsPlugins) {
  return async (params) => {
    const { STSClient, AssumeRoleCommand } = await import('@aws-sdk/client-sts');

    const stsClient = new STSClient({
      ...stsClientConfig,
      plugins: stsPlugins
    });

    const command = new AssumeRoleCommand(params);
    const response = await stsClient.send(command);

    return {
      accessKeyId: response.Credentials.AccessKeyId,
      secretAccessKey: response.Credentials.SecretAccessKey,
      sessionToken: response.Credentials.SessionToken,
      expiration: response.Credentials.Expiration
    };
  };
}

async function getDefaultRoleAssumerWithWebIdentity(stsClientConfig, stsPlugins) {
  return async (params) => {
    const { STSClient, AssumeRoleWithWebIdentityCommand } = await import('@aws-sdk/client-sts');

    const stsClient = new STSClient({
      ...stsClientConfig,
      plugins: stsPlugins
    });

    const command = new AssumeRoleWithWebIdentityCommand(params);
    const response = await stsClient.send(command);

    return {
      accessKeyId: response.Credentials.AccessKeyId,
      secretAccessKey: response.Credentials.SecretAccessKey,
      sessionToken: response.Credentials.SessionToken,
      expiration: response.Credentials.Expiration,
      ...(response.SubjectFromWebIdentityToken && {
        $source: {
          CREDENTIALS_STS_ASSUME_ROLE_WEB_ID: "i"
        }
      })
    };
  };
}

// Helper functions for credential providers
function parseKnownFiles(init) {
  const os = require('os');
  const path = require('path');
  const fs = require('fs');

  const credentialsPath = init?.filepath ?? path.join(os.homedir(), '.aws', 'credentials');
  const configPath = init?.configFilepath ?? path.join(os.homedir(), '.aws', 'config');

  const profiles = {};

  // Parse credentials file
  if (fs.existsSync(credentialsPath)) {
    const credContent = fs.readFileSync(credentialsPath, 'utf8');
    parseIniFile(credContent, profiles);
  }

  // Parse config file
  if (fs.existsSync(configPath)) {
    const configContent = fs.readFileSync(configPath, 'utf8');
    parseIniFile(configContent, profiles);
  }

  return profiles;
}

function parseIniFile(content, profiles) {
  const lines = content.split('\n');
  let currentProfile = null;

  for (const line of lines) {
    const trimmed = line.trim();

    // Skip comments and empty lines
    if (!trimmed || trimmed.startsWith('#') || trimmed.startsWith(';')) {
      continue;
    }

    // Check for profile header
    const profileMatch = trimmed.match(/^\[(.+)\]$/);
    if (profileMatch) {
      currentProfile = profileMatch[1].replace('profile ', '');
      if (!profiles[currentProfile]) {
        profiles[currentProfile] = {};
      }
      continue;
    }

    // Parse key-value pairs
    if (currentProfile) {
      const kvMatch = trimmed.match(/^([^=]+)=(.+)$/);
      if (kvMatch) {
        const key = kvMatch[1].trim();
        const value = kvMatch[2].trim();
        profiles[currentProfile][key] = value;
      }
    }
  }
}

function getProfileName(options) {
  return options?.profile ?? process.env.AWS_PROFILE ?? 'default';
}

// Utility functions for credential chain
function chain(...providers) {
  return async () => {
    let lastError;

    for (const provider of providers) {
      try {
        const credentials = await provider();
        if (credentials) return credentials;
      } catch (error) {
        lastError = error;
        if (error.tryNextLink === false) {
          throw error;
        }
      }
    }

    throw lastError || new Error('Could not load credentials from any providers');
  };
}

function memoize(provider) {
  let cached;
  let expiration;

  return async () => {
    if (cached && expiration && expiration > Date.now()) {
      return cached;
    }

    const credentials = await provider();
    cached = credentials;

    if (credentials.expiration) {
      expiration = credentials.expiration.getTime();
    } else {
      // Default to 15 minutes
      expiration = Date.now() + 15 * 60 * 1000;
    }

    return credentials;
  };
}

// Normalize provider to async function
function normalizeProvider(provider) {
  if (typeof provider === 'function') {
    return provider;
  }
  return async () => provider;
}

// Helper to set credential features
function setCredentialFeature(credentials, feature, value) {
  if (!credentials.$source) {
    credentials.$source = {};
  }
  credentials.$source[feature] = value;
}

// Retry helper for metadata services
function retry(fn, maxRetries) {
  const attempt = async (retriesLeft) => {
    try {
      return await fn();
    } catch (error) {
      if (retriesLeft > 0) {
        return attempt(retriesLeft - 1);
      }
      throw error;
    }
  };
  return attempt(maxRetries);
}

function generateUUID() {
  // Generate UUID v4
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    let r = Math.random() * 16 | 0;
    let v = c == 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

function serializeSession(session) {
  // Serialize session to JSON
  return {
    sid: session.sid,
    init: session.init,
    started: session.started,
    timestamp: session.timestamp,
    status: session.status,
    errors: session.errors,
    duration: session.duration,
    attrs: {
      release: session.release,
      environment: session.environment,
      ip_address: session.ipAddress,
      user_agent: session.userAgent,
    },
  };
}

// =============================================================================
// OAUTH FLOW FUNCTIONS
// =============================================================================

/**
 * Build OAuth authorization URL
 * From stringDecoder90 in test-fixed.js
 */
function buildOAuthAuthorizationUrl({
  codeChallenge,
  state,
  isManual,
  loginWithClaudeAi,
}) {
  let baseUrl = loginWithClaudeAi
    ? ANTHROPIC_CONFIG.CLAUDE_AI_AUTHORIZE_URL
    : ANTHROPIC_CONFIG.CONSOLE_AUTHORIZE_URL;
  
  let url = new URL(baseUrl);
  
  // CRITICAL: Parameters must be in this exact order
  url.searchParams.append("code", "true");
  url.searchParams.append("client_id", ANTHROPIC_CONFIG.CLIENT_ID);
  url.searchParams.append("response_type", "code");
  url.searchParams.append(
    "redirect_uri",
    isManual
      ? ANTHROPIC_CONFIG.MANUAL_REDIRECT_URL
      : `http://localhost:${ANTHROPIC_CONFIG.REDIRECT_PORT}/callback`
  );
  url.searchParams.append("scope", ANTHROPIC_CONFIG.SCOPES.join(" "));
  url.searchParams.append("code_challenge", codeChallenge);
  url.searchParams.append("code_challenge_method", "S256");
  url.searchParams.append("state", state);
  
  return url.toString();
}

/**
 * Exchange authorization code for tokens
 * From MvA in test-fixed.js
 */
async function exchangeCodeForTokens(code, state, codeVerifier, isManual = false) {
  let requestBody = {
    grant_type: "authorization_code",
    code: code,
    redirect_uri: isManual
      ? ANTHROPIC_CONFIG.MANUAL_REDIRECT_URL
      : `http://localhost:${ANTHROPIC_CONFIG.REDIRECT_PORT}/callback`,
    client_id: ANTHROPIC_CONFIG.CLIENT_ID,
    code_verifier: codeVerifier,
    state: state,
  };
  
  // Note: obj12.post would be the HTTP client in the actual implementation
  let response = await fetch(ANTHROPIC_CONFIG.TOKEN_URL, {
    method: 'POST',
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(requestBody)
  });
  
  return await response.json();
}

/**
 * Generate PKCE code verifier (32 random bytes, base64url encoded)
 * From func400 in test-fixed.js
 */
function generateCodeVerifier() {
  // In actual implementation, use crypto.randomBytes(32)
  // Then base64url encode it (stringDecoder454)
  let bytes = crypto.randomBytes(32);
  return base64UrlEncode(bytes);
}

/**
 * Generate PKCE code challenge from verifier
 * From stringDecoder455 in test-fixed.js
 */
function generateCodeChallenge(codeVerifier) {
  let hash = crypto.createHash("sha256");
  hash.update(codeVerifier);
  return base64UrlEncode(hash.digest());
}

/**
 * Generate random state parameter
 * From func401 in test-fixed.js
 */
function generateState() {
  return base64UrlEncode(crypto.randomBytes(32));
}

/**
 * Base64 URL encoding helper
 * From stringDecoder454 in test-fixed.js
 */
function base64UrlEncode(buffer) {
  return buffer
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=/g, "");
}

// =============================================================================
// SESSION MANAGEMENT - SENTRY INTEGRATION
// =============================================================================

/**
 * Core session capture function - Main API entry point
 * From checker316 in test-fixed.js (line 4812-4817)
 * This is the main captureSession function exported by the Sentry API
 */
function captureSession(endSession = false) {
  if (endSession) {
    endSessionInternal();
    return;
  }
  sendSessionUpdate();
}

/**
 * End session function - closes current session
 * From stringDecoder687 in test-fixed.js (line 4797-4802)
 */
function endSessionInternal() {
  let isolationScope = getCurrentHub().getIsolationScope(),
    currentScope = getCurrentScope(),
    session = currentScope.getSession() || isolationScope.getSession();
  if (session) closeSession(session);
  (sendSessionUpdate(), isolationScope.setSession(), currentScope.setSession());
}

/**
 * Send session update to client
 * From stringDecoder688 in test-fixed.js (line 4804-4810)
 */
function sendSessionUpdate() {
  let isolationScope = getCurrentHub().getIsolationScope(),
    currentScope = getCurrentScope(),
    client = getClient(),
    session = currentScope.getSession() || isolationScope.getSession();
  if (session && client && client.captureSession)
    client.captureSession(session);
}

/**
 * Start new session
 * From stringDecoder686 in test-fixed.js (line 4767-4795)
 * Creates a new session with proper metadata and user information
 */
function startSession(sessionData) {
  let client = getClient(),
    isolationScope = getCurrentHub().getIsolationScope(),
    currentScope = getCurrentScope(),
    {
      release: release,
      environment: environment = "production", // DEFAULT_ENVIRONMENT
    } = (client && client.getOptions()) || {},
    { userAgent: userAgent } = globalThis.navigator || {},
    newSession = makeSession({
      release: release,
      environment: environment,
      user: currentScope.getUser() || isolationScope.getUser(),
      ...(userAgent && {
        userAgent: userAgent,
      }),
      ...sessionData,
    }),
    existingSession = currentScope.getSession && currentScope.getSession();

  // Close existing session if it's still ok
  if (existingSession && existingSession.status === "ok")
    updateSession(existingSession, {
      status: "exited",
    });

  return (
    endSessionInternal(),
    isolationScope.setSession(newSession),
    currentScope.setSession(newSession),
    newSession
  );
}

/**
 * Hub class - Core session and scope management
 * From kc class in dH handler (line 5523-5805)
 * This is the central Hub that manages scopes, clients, and sessions
 */
class Hub {
  constructor(client, scope, isolationScope, version = 7.120) {
    this._version = version;
    let currentScope;
    if (!scope)
      ((currentScope = new Scope()),
        currentScope.setClient(client));
    else currentScope = scope;

    let currentIsolationScope;
    if (!isolationScope)
      ((currentIsolationScope = new Scope()), currentIsolationScope.setClient(client));
    else currentIsolationScope = isolationScope;

    if (
      ((this._stack = [
        {
          scope: currentScope,
        },
      ]),
      client)
    )
      this.bindClient(client);
    this._isolationScope = currentIsolationScope;
  }

  isOlderThan(version) {
    return this._version < version;
  }

  bindClient(client) {
    let stackTop = this.getStackTop();
    if (
      ((stackTop.client = client),
      stackTop.scope.setClient(client),
      client && client.setupIntegrations)
    )
      client.setupIntegrations();
  }

  pushScope() {
    let scope = this.getScope().clone();
    return (
      this.getStack().push({
        client: this.getClient(),
        scope: scope,
      }),
      scope
    );
  }

  popScope() {
    if (this.getStack().length <= 1) return false;
    return !!this.getStack().pop();
  }

  withScope(callback) {
    let scope = this.pushScope(),
      result;
    try {
      result = callback(scope);
    } catch (error) {
      throw (this.popScope(), error);
    }
    if (isThenable(result))
      return result.then(
        (value) => {
          return (this.popScope(), value);
        },
        (error) => {
          throw (this.popScope(), error);
        },
      );
    return (this.popScope(), result);
  }

  getClient() {
    return this.getStackTop().client;
  }

  getScope() {
    return this.getStackTop().scope;
  }

  getIsolationScope() {
    return this._isolationScope;
  }

  getStack() {
    return this._stack;
  }

  getStackTop() {
    return this._stack[this._stack.length - 1];
  }

  /**
   * Session capture at Hub level
   * From line 5744-5747 in Hub class
   */
  captureSession(endSession = false) {
    if (endSession) return this.endSession();
    this._sendSessionUpdate();
  }

  /**
   * End session at Hub level
   * From line 5748-5753 in Hub class
   */
  endSession() {
    let scope = this.getStackTop().scope,
      session = scope.getSession();
    if (session) closeSession(session);
    (this._sendSessionUpdate(), scope.setSession());
  }

  /**
   * Start session at Hub level
   * From line 5754-5779 in Hub class
   */
  startSession(sessionData) {
    let { scope: scope, client: client } = this.getStackTop(),
      {
        release: release,
        environment: environment = "production",
      } = (client && client.getOptions()) || {},
      { userAgent: userAgent } = globalThis.navigator || {},
      newSession = makeSession({
        release: release,
        environment: environment,
        user: scope.getUser(),
        ...(userAgent && {
          userAgent: userAgent,
        }),
        ...sessionData,
      }),
      existingSession = scope.getSession && scope.getSession();
    if (existingSession && existingSession.status === "ok")
      updateSession(existingSession, {
        status: "exited",
      });
    return (
      this.endSession(),
      scope.setSession(newSession),
      newSession
    );
  }

  /**
   * Send session update to client
   * From line 5786-5791 in Hub class
   */
  _sendSessionUpdate() {
    let { scope: scope, client: client } = this.getStackTop(),
      session = scope.getSession();
    if (session && client && client.captureSession)
      client.captureSession(session);
  }

  captureException(exception, hint) {
    let eventId = (this._lastEventId =
        hint && hint.event_id
          ? hint.event_id
          : generateUUID()),
      syntheticException = new Error("Sentry syntheticException");
    return (
      this.getScope().captureException(exception, {
        originalException: exception,
        syntheticException: syntheticException,
        ...hint,
        event_id: eventId,
      }),
      eventId
    );
  }

  captureMessage(message, level, hint) {
    let eventId = (this._lastEventId =
        hint && hint.event_id ? hint.event_id : generateUUID()),
      syntheticException = new Error(message);
    return (
      this.getScope().captureMessage(message, level, {
        originalException: message,
        syntheticException: syntheticException,
        ...hint,
        event_id: eventId,
      }),
      eventId
    );
  }

  lastEventId() {
    return this._lastEventId;
  }

  setUser(user) {
    (this.getScope().setUser(user),
      this.getIsolationScope().setUser(user));
  }

  setTags(tags) {
    (this.getScope().setTags(tags),
      this.getIsolationScope().setTags(tags));
  }

  setExtras(extras) {
    (this.getScope().setExtras(extras),
      this.getIsolationScope().setExtras(extras));
  }

  setTag(key, value) {
    (this.getScope().setTag(key, value),
      this.getIsolationScope().setTag(key, value));
  }

  setExtra(key, extra) {
    (this.getScope().setExtra(key, extra),
      this.getIsolationScope().setExtra(key, extra));
  }

  setContext(key, context) {
    (this.getScope().setContext(key, context),
      this.getIsolationScope().setContext(key, context));
  }

  configureScope(callback) {
    let { scope: scope, client: client } = this.getStackTop();
    if (client) callback(scope);
  }
}

/**
 * Global Hub Management Functions
 * From dH handler exports (line 5872-5883)
 */

// Global hub carrier
function getMainCarrier() {
  return (
    (globalThis.__SENTRY__ = globalThis.__SENTRY__ || {
      extensions: {},
      hub: undefined,
    }),
    globalThis
  );
}

// Get current hub from carrier
function getHubFromCarrier(carrier) {
  return getGlobalSingleton("hub", () => new Hub(), carrier);
}

// Set hub on carrier
function setHubOnCarrier(carrier, hub) {
  if (!carrier) return false;
  let sentryData = (carrier.__SENTRY__ = carrier.__SENTRY__ || {});
  return ((sentryData.hub = hub), true);
}

// Make hub the main hub
function makeMainHub(hub) {
  let carrier = getMainCarrier(),
    oldHub = getHubFromCarrier(carrier);
  return (setHubOnCarrier(carrier, hub), oldHub);
}

// Check if carrier has hub
function hasHubOnCarrier(carrier) {
  return !!(carrier && carrier.__SENTRY__ && carrier.__SENTRY__.hub);
}

// Get current hub (main API entry point)
function getCurrentHub() {
  let carrier = getMainCarrier();
  if (carrier.__SENTRY__ && carrier.__SENTRY__.acs) {
    let hub = carrier.__SENTRY__.acs.getCurrentHub();
    if (hub) return hub;
  }
  return getIsolationScopeHub(carrier);
}

// Ensure hub on carrier
function ensureHubOnCarrier(carrier, parentHub = getIsolationScopeHub()) {
  if (!hasHubOnCarrier(carrier) || getHubFromCarrier(carrier).isOlderThan(7.120)) {
    let client = parentHub.getClient(),
      scope = parentHub.getScope(),
      isolationScope = parentHub.getIsolationScope();
    setHubOnCarrier(
      carrier,
      new Hub(client, scope.clone(), isolationScope.clone()),
    );
  }
}

// Get isolation scope hub
function getIsolationScopeHub(carrier = getMainCarrier()) {
  if (!hasHubOnCarrier(carrier) || getHubFromCarrier(carrier).isOlderThan(7.120))
    setHubOnCarrier(carrier, new Hub());
  return getHubFromCarrier(carrier);
}

// Get isolation scope
function getIsolationScope() {
  return getCurrentHub().getIsolationScope();
}

/**
 * Core API Functions
 * From main Sentry API exports (line 4758-4765)
 */

// Get current client
function getClient() {
  return getCurrentHub().getClient();
}

// Get current scope
function getCurrentScope() {
  return getCurrentHub().getScope();
}

/**
 * Client Session Management
 * From CQA class in test-fixed.js (line 7942-7953 and 8051-8059)
 */
class SentryClient {
  constructor(options) {
    this._options = options;
  }

  /**
   * Capture session at client level
   * From line 7942-7953 in client class
   */
  captureSession(session) {
    if (typeof session.release !== "string")
      console.warn(
        "Discarded session because of missing or non-string release",
      );
    else
      (this.sendSession(session),
        updateSession(session, {
          init: false,
        }));
  }

  /**
   * Send session envelope to transport
   * From line 8051-8059 in client class
   */
  sendSession(session) {
    let envelope = createSessionEnvelope(
      session,
      this._dsn,
      this._options._metadata,
      this._options.tunnel,
    );
    this._sendEnvelope(envelope);
  }

  getOptions() {
    return this._options;
  }

  _sendEnvelope(envelope) {
    // Transport implementation would go here
    if (this._transport) {
      this._transport.send(envelope);
    }
  }
}

/**
 * Session Envelope Creation
 * From func607 in test-fixed.js (line 7394-7421)
 * Creates proper Sentry envelopes for session data
 */
function createSessionEnvelope(session, dsn, metadata, tunnel) {
  let sdkMetadata = getSdkMetadataForEnvelopeHeader(metadata),
    envelopeHeader = {
      sent_at: new Date().toISOString(),
      ...(sdkMetadata && {
        sdk: sdkMetadata,
      }),
      ...(tunnel &&
        dsn && {
          dsn: dsnToString(dsn),
        }),
    },
    envelopeItem =
      "aggregates" in session
        ? [
            {
              type: "sessions",
            },
            session,
          ]
        : [
            {
              type: "session",
            },
            session.toJSON(),
          ];
  return createEnvelope(envelopeHeader, [envelopeItem]);
}

/**
 * Session Aggregates Manager
 * From AQA class in test-fixed.js (line 7453-7485)
 * Manages batched session reporting
 */
class SessionAggregatesManager {
  constructor(client, sessionAttrs) {
    this._client = client;
    this.flushTimeout = 60;
    this._pendingAggregates = {};
    this._isEnabled = true;
    this._intervalId = setInterval(
      () => this.flush(),
      this.flushTimeout * 1000,
    );
    if (this._intervalId.unref)
      this._intervalId.unref();
    this._sessionAttrs = sessionAttrs;
  }

  flush() {
    let sessionAggregates = this.getSessionAggregates();
    if (sessionAggregates.aggregates.length === 0) return;
    ((this._pendingAggregates = {}), this._client.sendSession(sessionAggregates));
  }

  getSessionAggregates() {
    let aggregates = Object.keys(this._pendingAggregates).map((key) => {
        return this._pendingAggregates[parseInt(key)];
      }),
      sessionData = {
        attrs: this._sessionAttrs,
        aggregates: aggregates,
      };
    return dropUndefinedKeys(sessionData);
  }

  close() {
    (clearInterval(this._intervalId),
      (this._isEnabled = false),
      this.flush());
  }
}

/**
 * Scope class for session management
 * This would be implemented as part of the scope system
 */
class Scope {
  constructor() {
    this._session = null;
    this._user = null;
    this._tags = {};
    this._extras = {};
    this._contexts = {};
    this._client = null;
  }

  setSession(session) {
    this._session = session;
  }

  getSession() {
    return this._session;
  }

  setUser(user) {
    this._user = user;
  }

  getUser() {
    return this._user;
  }

  setClient(client) {
    this._client = client;
  }

  getClient() {
    return this._client;
  }

  setTags(tags) {
    this._tags = { ...this._tags, ...tags };
  }

  setTag(key, value) {
    this._tags[key] = value;
  }

  setExtras(extras) {
    this._extras = { ...this._extras, ...extras };
  }

  setExtra(key, value) {
    this._extras[key] = value;
  }

  setContext(key, context) {
    this._contexts[key] = context;
  }

  clone() {
    let cloned = new Scope();
    cloned._session = this._session;
    cloned._user = this._user;
    cloned._tags = { ...this._tags };
    cloned._extras = { ...this._extras };
    cloned._contexts = { ...this._contexts };
    cloned._client = this._client;
    return cloned;
  }

  captureException(exception, hint) {
    if (this._client) {
      return this._client.captureException(exception, hint);
    }
  }

  captureMessage(message, level, hint) {
    if (this._client) {
      return this._client.captureMessage(message, level, hint);
    }
  }
}

/**
 * Utility Functions
 */

// Check if value is thenable (Promise-like)
function isThenable(wat) {
  return Boolean(wat && wat.then && typeof wat.then === 'function');
}

// Get global singleton
function getGlobalSingleton(name, creator, carrier) {
  carrier = carrier || globalThis;
  if (!carrier.__SENTRY_GLOBALS__) {
    carrier.__SENTRY_GLOBALS__ = {};
  }
  if (!carrier.__SENTRY_GLOBALS__[name]) {
    carrier.__SENTRY_GLOBALS__[name] = creator();
  }
  return carrier.__SENTRY_GLOBALS__[name];
}

// Drop undefined keys from object
function dropUndefinedKeys(inputValue) {
  const output = {};
  for (const key in inputValue) {
    if (inputValue[key] !== undefined) {
      output[key] = inputValue[key];
    }
  }
  return output;
}

// Create envelope (simplified)
function createEnvelope(headers, items) {
  return {
    headers: headers,
    items: items
  };
}

// Get SDK metadata for envelope header (stub)
function getSdkMetadataForEnvelopeHeader(metadata) {
  return metadata?.sdk || null;
}

// Convert DSN to string (stub)
function dsnToString(dsn) {
  return dsn?.toString?.() || dsn;
}

// =============================================================================
// EXPORTS
// =============================================================================

module.exports = {
  // HTTP Authentication
  HttpApiKeyAuthLocation,
  HttpRequest,
  DefaultIdentityProviderConfig,
  HttpApiKeyAuthSigner,
  HttpBearerAuthSigner,
  NoAuthSigner,
  
  // Anthropic Authentication
  AnthropicClient,
  resolveAnthropicApiKey,
  checkTokenAvailability,
  getAnthropicApiKey,
  setupBearerToken,
  getCustomHeaders,
  buildAuthHeaders,
  fetchOAuthProfile,
  truncateApiKey,
  
  // AWS Authentication - Core
  ChecksumAlgorithm,
  checkAWSAuthFeatures,
  getSSOTokenFromFile,
  getResolvedSigningRegion,
  createChecksumConfiguration,
  getSSOTokenFilepath,

  // AWS Credential Providers
  fromEnv,
  fromContainerMetadata,
  fromInstanceMetadata,
  fromSSO,
  fromIni,
  fromProcess,
  fromWebToken,
  fromTokenFile,
  defaultProvider,

  // AWS SigV4 Signing
  SignatureV4,

  // AWS Role Assumers
  getDefaultRoleAssumer,
  getDefaultRoleAssumerWithWebIdentity,

  // AWS Helper Functions
  parseKnownFiles,
  parseIniFile,
  getProfileName,
  chain,
  memoize,
  normalizeProvider,
  setCredentialFeature,
  retry,
  
  // Session Management - Original
  makeSession,
  updateSession,
  closeSession,
  SessionManager,

  // Session Management - Sentry Integration
  captureSession,
  endSessionInternal,
  sendSessionUpdate,
  startSession,
  Hub,
  SentryClient,
  SessionAggregatesManager,
  Scope,

  // Hub Management
  getCurrentHub,
  getCurrentScope,
  getClient,
  getIsolationScope,
  getMainCarrier,
  getHubFromCarrier,
  setHubOnCarrier,
  makeMainHub,
  hasHubOnCarrier,
  ensureHubOnCarrier,
  getIsolationScopeHub,

  // Session Utilities
  createSessionEnvelope,
  isThenable,
  getGlobalSingleton,
  dropUndefinedKeys,
  createEnvelope,
  getSdkMetadataForEnvelopeHeader,
  dsnToString,

  // Client Management
  ClientManager,
  
  // Proxy Authentication
  addProxyAuthentication,
  
  // OAuth Flow Functions
  buildOAuthAuthorizationUrl,
  exchangeCodeForTokens,
  generateCodeVerifier,
  generateCodeChallenge,
  generateState,
  base64UrlEncode,
  getSubscriptionType,
  determineOAuthEndpoint,
  
  // Credential Storage Functions
  getCredentialStorage,
  createKeychainStorage,
  createPlaintextStorage,
  createFallbackStorage,
  getServiceName,
  getConfigPath,
  
  // Utility Functions
  getOAuthToken,
  hasValidScopes,
  hasOAuthAccess,
  setFeature,
  getSSOTokenFilepath,
  generateUUID,
  serializeSession,
  
  // Configuration
  ANTHROPIC_CONFIG,
};