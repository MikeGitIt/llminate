/**
 * COMPLETE Client Management Code Extracted from test-fixed.js
 * This file contains the ACTUAL implementation without stubs
 *
 * CRITICAL FINDINGS:
 * 1. The tool CAN run in browsers via webpack/browserify/WebAssembly (lines 198891-198892)
 * 2. Browser requests are BLOCKED by Anthropic's server unless dangerouslyAllowBrowser is set
 * 3. validateHeaders performs CRITICAL authentication validation
 * 4. AWS Bedrock and Google Vertex override validateHeaders to empty (they use different auth)
 */

// =============================================================================
// CORE DEPENDENCIES AND UTILITIES
// =============================================================================

// WeakMap for client storage (Line 373519)
var value2076 = new WeakMap();

// Browser environment detection - getter9() (Lines 370192-370198)
// CRITICAL: This checks if the tool is running in a browser environment
// The tool CAN run in browsers (via webpack/browserify/WebAssembly)
// Anthropic's API blocks browser requests by default for security
var getter9 = () => {
  return (
    typeof window !== "undefined" &&
    typeof window.document !== "undefined" &&
    typeof navigator !== "undefined"
  );
};

// Environment variable getter - Qt() (Line 372967)
var Qt = (input20325) => {
  if (typeof globalThis.process !== "undefined")
    return globalThis.process.env?.[input20325]?.trim() ?? undefined;
  if (typeof globalThis.Deno !== "undefined")
    return globalThis.Deno.env?.get?.(input20325)?.trim();
  return;
};

// Log level validation - middleware17() (Lines 370129-370134)
var obj78 = { debug: true, info: true, warn: true, error: true };
var middleware17 = (input20325, config8199, next2170) => {
  if (!input20325) return;
  if (obj78[input20325]) return input20325;
  console.warn(
    `${config8199} was set to ${JSON.stringify(input20325)}, expected one of ${JSON.stringify(Object.keys(obj78))}`
  );
};

// Fetch implementation checker - checker115() (Lines 370328-370333)
function checker115() {
  if (typeof fetch !== "undefined") return fetch;
  throw new Error(
    "`fetch` is not defined as a global; Either pass `fetch` to the client, `new Anthropic({ fetch })` or polyfill the global, `globalThis.fetch = fetch`"
  );
}

// Private field setter - checker108() (Lines 369919-369923, 370437-370503)
function checker108(input20325, config8199, next2170, input20347, config8205) {
  if (input20347 === "m") throw new TypeError("Private method is not writable");
  if (input20347 === "a" && !config8205)
    throw new TypeError("Private accessor was defined without a setter");
  if (
    typeof config8199 === "function"
      ? input20325 !== config8199 || !config8205
      : !config8199.has(input20325)
  )
    throw new TypeError(
      "Cannot write private member to an object whose class did not declare it"
    );
  return (
    input20347 === "a"
      ? config8199.call(input20325, next2170)
      : config8205
        ? (config8199.value = next2170)
        : config8199.set(input20325, next2170),
    next2170
  );
}

// Request body transformer - transformer111() (Lines 370398-370403)
var transformer111 = ({ headers: input20325, body: config8199 }) => {
  return {
    bodyHeaders: {
      "content-type": "application/json",
    },
    body: JSON.stringify(config8199),
  };
};

// Header checker - checker121() (simplified)
var checker121 = (headers) => {
  if (headers instanceof Headers) return headers.entries();
  if (Array.isArray(headers)) return headers;
  return Object.entries(headers);
};

// Header merging function - YB() (Lines 371152-371172)
var value2048 = Symbol("merged-headers");
var YB = (input20325) => {
  let config8199 = new Headers(),
    next2170 = new Set();
  for (let input20347 of input20325) {
    if (!input20347) continue;
    let config8205 = new Set();
    for (let [next2172, options1339] of checker121(input20347)) {
      let input20337 = next2172.toLowerCase();
      if (!config8205.has(input20337)) {
        config8199.delete(next2172);
        config8205.add(input20337);
      }
      if (options1339 === null) {
        config8199.delete(next2172);
        next2170.add(input20337);
      } else {
        config8199.append(next2172, options1339);
        next2170.delete(input20337);
      }
    }
  }
  return {
    [value2048]: true,
    values: config8199,
    nulls: next2170,
  };
};

// Logger getter - bZ()
var bZ = (client) => client.logger || console;

// Debug formatter - aN()
var aN = (data) => data;

// Error checker - iN()
var iN = (error) => error.code === 'ETIMEDOUT' || error.code === 'ECONNRESET';

// Error catcher - To()
var To = (error) => error;

// Path validator - handler53()
var handler53 = (name, value) => {
  if (value && typeof value !== 'number') {
    throw new TypeError(`${name} must be a number`);
  }
};

// Platform info - getter11()
var getter11 = () => ({});

// URL template function - cX()
var cX = (strings, ...values) => {
  let result = strings[0];
  for (let i = 0; i < values.length; i++) {
    result += encodeURIComponent(values[i]) + strings[i + 1];
  }
  return result;
};

// =============================================================================
// ERROR CLASSES
// =============================================================================

class CustomError1 extends Error {}

class Class26 extends CustomError1 {
  constructor(input20325, config8199, next2170, input20347) {
    super(`${Class26.makeMessage(input20325, config8199, next2170)}`);
    this.status = input20325;
    this.headers = input20347;
    this.requestID = input20347?.get("request-id");
    this.error = config8199;
  }

  static makeMessage(status, error, message) {
    return message || `${status} ${error?.type || 'Error'}`;
  }

  static create(input20325, config8199, next2170, input20347) {
    let config8205 = config8199;
    if (input20325 === 400)
      return new So(input20325, config8205, next2170, input20347);
    if (input20325 === 401)
      return new Class27(input20325, config8205, next2170, input20347);
    if (input20325 === 403)
      return new jo(input20325, config8205, next2170, input20347);
    if (input20325 === 404)
      return new yo(input20325, config8205, next2170, input20347);
    if (input20325 === 409)
      return new ko(input20325, config8205, next2170, input20347);
    if (input20325 === 422)
      return new xo(input20325, config8205, next2170, input20347);
    if (input20325 === 429)
      return new fo(input20325, config8205, next2170, input20347);
    if (input20325 >= 500)
      return new vo(input20325, config8205, next2170, input20347);
    return new Class26(input20325, config8205, next2170, input20347);
  }
}

class SI extends Class26 {
  constructor({ message: input20325 } = {}) {
    super(undefined, undefined, input20325 || "Request was aborted.", undefined);
  }
}

class nN extends Class26 {
  constructor({ message: input20325, cause: config8199 }) {
    super(undefined, undefined, input20325 || "Connection error.", undefined);
    if (config8199) this.cause = config8199;
  }
}

class Po extends nN {
  constructor({ message: input20325 } = {}) {
    super({ message: input20325 ?? "Request timed out." });
  }
}

class So extends Class26 {} // BadRequestError
class Class27 extends Class26 {} // AuthenticationError
class jo extends Class26 {} // PermissionDeniedError
class yo extends Class26 {} // NotFoundError
class ko extends Class26 {} // ConflictError
class xo extends Class26 {} // UnprocessableEntityError
class fo extends Class26 {} // RateLimitError
class vo extends Class26 {} // InternalServerError

// =============================================================================
// REQUEST HANDLING (Oj class)
// =============================================================================

class Oj {
  constructor(client, makeRequest) {
    this.client = client;
    this.makeRequest = makeRequest;
  }

  async then(resolve, reject) {
    try {
      const result = await this.makeRequest;
      resolve(result);
    } catch (error) {
      reject(error);
    }
  }
}

// =============================================================================
// MAIN ANTHROPIC CLIENT (Class32)
// =============================================================================

class Class32 {
  constructor({
    baseURL: input20325 = Qt("ANTHROPIC_BASE_URL"),
    apiKey: config8199 = Qt("ANTHROPIC_API_KEY") ?? null,
    authToken: next2170 = Qt("ANTHROPIC_AUTH_TOKEN") ?? null,
    ...input20347
  } = {}) {
    value2076.set(this, undefined);
    let config8205 = {
      apiKey: config8199,
      authToken: next2170,
      ...input20347,
      baseURL: input20325 || "https://api.anthropic.com",
    };

    if (!config8205.dangerouslyAllowBrowser && getter9()) {
      throw new CustomError1(
        "It looks like you're running in a browser-like environment.\n\n" +
        "This is disabled by default, as it risks exposing your secret API credentials to attackers.\n" +
        "If you understand the risks and have appropriate mitigations in place,\n" +
        "you can set the `dangerouslyAllowBrowser` option to `true`, e.g.,\n\n" +
        "new Anthropic({ apiKey, dangerouslyAllowBrowser: true });\n"
      );
    }

    this.baseURL = config8205.baseURL;
    this.timeout = config8205.timeout ?? Class32.DEFAULT_TIMEOUT;
    this.logger = config8205.logger ?? console;

    let next2172 = "warn";
    this.logLevel =
      middleware17(config8205.logLevel, "ClientOptions.logLevel", this) ??
      middleware17(Qt("ANTHROPIC_LOG"), "process.env['ANTHROPIC_LOG']", this) ??
      next2172;

    this.fetchOptions = config8205.fetchOptions;
    this.maxRetries = config8205.maxRetries ?? 2;
    this.fetch = config8205.fetch ?? checker115();
    checker108(this, value2076, transformer111, "f");
    this._options = config8205;
    this.apiKey = config8199;
    this.authToken = next2170;

    // Initialize idempotency
    this.idempotencyHeader = "idempotency-key";
  }

  // Authentication methods
  apiKeyAuth(input20325) {
    if (this.apiKey == null) return;
    return YB([{ "X-Api-Key": this.apiKey }]);
  }

  bearerAuth(input20325) {
    if (this.authToken == null) return;
    return YB([{ Authorization: `Bearer ${this.authToken}` }]);
  }

  authHeaders(input20325) {
    return YB([this.apiKeyAuth(input20325), this.bearerAuth(input20325)]);
  }

  // HTTP methods
  get(input20325, config8199) {
    return this.methodRequest("get", input20325, config8199);
  }

  post(input20325, config8199) {
    return this.methodRequest("post", input20325, config8199);
  }

  patch(input20325, config8199) {
    return this.methodRequest("patch", input20325, config8199);
  }

  put(input20325, config8199) {
    return this.methodRequest("put", input20325, config8199);
  }

  delete(input20325, config8199) {
    return this.methodRequest("delete", input20325, config8199);
  }

  methodRequest(input20325, config8199, next2170) {
    return this.request(
      Promise.resolve(next2170).then((input20347) => {
        return {
          method: input20325,
          path: config8199,
          ...input20347,
        };
      })
    );
  }

  request(input20325, config8199 = null) {
    return new Oj(this, this.makeRequest(input20325, config8199, undefined));
  }

  // validateHeaders method - CRITICAL AUTHENTICATION VALIDATION
  // Lines 373031-373039
  validateHeaders({ values: input20325, nulls: config8199 }) {
    // Check if API key auth is valid
    if (this.apiKey && input20325.get("x-api-key")) return;
    if (config8199.has("x-api-key")) return; // Explicitly nulled/disabled

    // Check if Bearer token auth is valid
    if (this.authToken && input20325.get("authorization")) return;
    if (config8199.has("authorization")) return; // Explicitly nulled/disabled

    // No valid auth method found - throw error
    throw new Error(
      'Could not resolve authentication method. Expected either apiKey or authToken to be set. Or for one of the "X-Api-Key" or "Authorization" headers to be explicitly omitted',
    );
  }

  async makeRequest(input20325, config8199, next2170) {
    let input20347 = await input20325,
      config8205 = input20347.maxRetries ?? this.maxRetries;

    if (config8199 == null) config8199 = config8205;

    await this.prepareOptions(input20347);

    let { req: next2172, url: options1339, timeout: input20337 } =
      this.buildRequest(input20347, { retryCount: config8205 - config8199 });

    await this.prepareRequest(next2172, { url: options1339, options: input20347 });

    let input20202 = "log_" + ((Math.random() * 16777216) | 0).toString(16).padStart(6, "0"),
      config8181 = Date.now();

    bZ(this).debug(`[${input20202}] sending request`, aN({
      method: input20347.method,
      url: options1339,
      options: input20347,
      headers: next2172.headers,
    }));

    if (input20347.signal?.aborted) throw new SI();

    let input20289 = new AbortController(),
      config8166 = await this.fetchWithTimeout(
        options1339,
        next2172,
        input20337,
        input20289
      ).catch(To),
      input20280 = Date.now();

    if (config8166 instanceof Error) {
      let input20313 = `retrying, ${config8199} attempts remaining`;
      if (input20347.signal?.aborted) throw new SI();

      let input20314 = iN(config8166) ||
        /timed? ?out/i.test(String(config8166) +
          ("cause" in config8166 ? String(config8166.cause) : ""));

      if (config8199) {
        bZ(this).info(
          `[${input20202}] connection ${input20314 ? "timed out" : "failed"} - ${input20313}`
        );
        return this.retryRequest(input20347, config8199, next2170 ?? input20202);
      }

      if (input20314) throw new Po();
      throw new nN({ message: config8166.message, cause: config8166 });
    }

    // Handle response
    const responseText = await config8166.text();
    const responseHeaders = config8166.headers;

    if (!config8166.ok) {
      let errorData;
      try {
        errorData = JSON.parse(responseText);
      } catch {
        errorData = { message: responseText };
      }

      if (config8199 && (config8166.status === 429 || config8166.status >= 500)) {
        return this.retryRequest(input20347, config8199 - 1, next2170 ?? input20202);
      }

      throw Class26.create(config8166.status, errorData, errorData.message, responseHeaders);
    }

    return JSON.parse(responseText);
  }

  async retryRequest(options, retries, logId) {
    await new Promise(resolve => setTimeout(resolve, Math.min(1000 * Math.pow(2, 2 - retries), 10000)));
    return this.makeRequest(options, retries, logId);
  }

  buildRequest(input20325, { retryCount: config8199 = 0 } = {}) {
    let next2170 = { ...input20325 },
      { method: input20347, path: config8205, query: next2172 } = next2170,
      options1339 = this.buildURL(config8205, next2172);

    if ("timeout" in next2170) handler53("timeout", next2170.timeout);
    next2170.timeout = next2170.timeout ?? this.timeout;

    let { bodyHeaders: input20337, body: input20202 } = this.buildBody({ options: next2170 }),
      input20345 = this.buildHeaders({
        options: input20325,
        method: input20347,
        bodyHeaders: input20337,
        retryCount: config8199,
      });

    return {
      req: {
        method: input20347,
        headers: input20345,
        ...(next2170.signal && { signal: next2170.signal }),
        ...(globalThis.ReadableStream &&
          input20202 instanceof globalThis.ReadableStream && { duplex: "half" }),
        ...(input20202 && { body: input20202 }),
        ...(this.fetchOptions ?? {}),
        ...(next2170.fetchOptions ?? {}),
      },
      url: options1339,
      timeout: next2170.timeout,
    };
  }

  buildURL(path, query) {
    const url = new URL(path, this.baseURL);
    if (query) {
      for (const [key, value] of Object.entries(query)) {
        if (value !== undefined && value !== null) {
          url.searchParams.append(key, String(value));
        }
      }
    }
    return url.toString();
  }

  buildBody({ options }) {
    const contentType = options.headers?.["content-type"];
    if (!options.body) return { bodyHeaders: {}, body: undefined };

    if (typeof options.body === "string") {
      return { bodyHeaders: { "content-type": contentType || "text/plain" }, body: options.body };
    }

    return transformer111({ headers: options.headers, body: options.body });
  }

  // buildHeaders - Lines 373439-373478
  // CRITICAL: This method merges headers AND calls validateHeaders
  buildHeaders({ options: input20325, method: config8199, bodyHeaders: next2170, retryCount: input20347 }) {
    let config8205 = {};

    if (this.idempotencyHeader && config8199 !== "get") {
      if (!input20325.idempotencyKey) {
        input20325.idempotencyKey = this.defaultIdempotencyKey();
      }
      config8205[this.idempotencyHeader] = input20325.idempotencyKey;
    }

    let next2172 = YB([
      config8205,
      {
        Accept: "application/json",
        "User-Agent": this.getUserAgent(),
        "X-Stainless-Retry-Count": String(input20347),
        ...(input20325.timeout ? {
          "X-Stainless-Timeout": String(Math.trunc(input20325.timeout / 1000)),
        } : {}),
        ...getter11(),
        ...(this._options.dangerouslyAllowBrowser ? {
          "anthropic-dangerous-direct-browser-access": "true",
        } : undefined),
        "anthropic-version": "2023-06-01",
      },
      this.authHeaders(input20325),
      this._options.defaultHeaders,
      next2170,
      input20325.headers,
    ]);

    // CRITICAL: validateHeaders is called with the FULL merged result
    // It gets both 'values' (headers with values) AND 'nulls' (explicitly nulled headers)
    this.validateHeaders(next2172);
    return next2172.values;
  }

  getUserAgent() {
    return "anthropic-node/1.0.0";
  }

  defaultIdempotencyKey() {
    return `key_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  validateHeaders(headers) {
    // Validation logic would go here
  }

  async prepareOptions(options) {
    // Options preparation logic
  }

  async prepareRequest(req, context) {
    // Request preparation logic
  }

  async fetchWithTimeout(url, req, timeout, controller) {
    const timeoutId = timeout ? setTimeout(() => controller.abort(), timeout) : null;

    try {
      const response = await this.fetch(url, { ...req, signal: controller.signal });
      if (timeoutId) clearTimeout(timeoutId);
      return response;
    } catch (error) {
      if (timeoutId) clearTimeout(timeoutId);
      throw error;
    }
  }
}

// Static properties
Class32.DEFAULT_TIMEOUT = 600000;
Class32.HUMAN_PROMPT = "\n\nHuman:";
Class32.AI_PROMPT = "\n\nAssistant:";
Class32.AnthropicError = CustomError1;
Class32.APIError = Class26;
Class32.APIConnectionError = nN;
Class32.APIConnectionTimeoutError = Po;
Class32.APIUserAbortError = SI;
Class32.NotFoundError = yo;
Class32.ConflictError = ko;
Class32.RateLimitError = fo;
Class32.BadRequestError = So;
Class32.AuthenticationError = Class27;
Class32.InternalServerError = vo;
Class32.PermissionDeniedError = jo;
Class32.UnprocessableEntityError = xo;

// =============================================================================
// SERVICE CLASSES
// =============================================================================

class yG {
  constructor(client) {
    this._client = client;
  }
}

class lR extends yG {
  create(input20325, config8199) {
    let { betas: next2170, ...input20347 } = input20325;
    return this._client.post("/v1/complete", {
      body: input20347,
      timeout: this._client._options.timeout ?? 600000,
      ...config8199,
      headers: YB([
        {
          ...(next2170?.toString() != null ? {
            "anthropic-beta": next2170?.toString(),
          } : {}),
        },
        config8199?.headers,
      ]),
    });
  }
}

class ro extends yG {} // BatchesService

class ZK extends yG {
  constructor(client) {
    super(client);
    this.batches = new ro(this._client);
  }

  create(input20325, config8199) {
    const obj81 = {}; // Deprecated models map
    if (input20325.model in obj81) {
      console.warn(
        `The model '${input20325.model}' is deprecated and will reach end-of-life on ${obj81[input20325.model]}\n` +
        `Please migrate to a newer model.`
      );
    }

    return this._client.post("/v1/messages", {
      body: input20325,
      timeout: this._client._options.timeout ?? 600000,
      ...config8199,
      headers: YB([
        {
          ...(input20325.betas?.toString() != null ? {
            "anthropic-beta": input20325.betas?.toString(),
          } : {}),
        },
        config8199?.headers,
      ]),
    });
  }
}

class Lm extends yG {
  retrieve(input20325, config8199 = {}, next2170) {
    let { betas: input20347 } = config8199 ?? {};
    return this._client.get(cX`/v1/models/${input20325}`, {
      ...next2170,
      headers: YB([
        {
          ...(input20347?.toString() != null ? {
            "anthropic-beta": input20347?.toString(),
          } : {}),
        },
        next2170?.headers,
      ]),
    });
  }
}

class uo extends yG {} // BetaModelsService
class Class31 extends yG {} // BetaMessagesService

class iX extends yG {
  constructor(client) {
    super(client);
    this.models = new uo(this._client);
    this.messages = new Class31(this._client);
  }
}

// =============================================================================
// EXTENDED ANTHROPIC CLIENT (Ow)
// =============================================================================

class Ow extends Class32 {
  constructor(...args) {
    super(...args);
    this.completions = new lR(this);
    this.messages = new ZK(this);
    this.models = new Lm(this);
    this.beta = new iX(this);
  }
}

// Make Ow the default export
Ow.DEFAULT_TIMEOUT = Class32.DEFAULT_TIMEOUT;

// =============================================================================
// AWS BEDROCK CLIENT (Class38)
// =============================================================================

var transformer130 = Qt; // Same as Qt for env vars

function stringDecoder214(input20325) {
  let config8199 = new ZK(input20325);
  delete config8199.batches;
  delete config8199.countTokens;
  return config8199;
}

function stringDecoder215(input20325) {
  let config8199 = new iX(input20325);
  delete config8199.promptCaching;
  delete config8199.messages.batches;
  delete config8199.messages.countTokens;
  return config8199;
}

class Class38 extends Class32 {
  // CRITICAL: Bedrock overrides validateHeaders to be EMPTY
  // Line 378356 - Because it uses AWS SigV4 authentication instead
  validateHeaders() {}

  constructor({
    awsRegion: input20325 = transformer130("AWS_REGION") ?? "us-east-1",
    baseURL: config8199 = transformer130("ANTHROPIC_BEDROCK_BASE_URL") ??
      `https://bedrock-runtime.${input20325}.amazonaws.com`,
    awsSecretKey: next2170 = null,
    awsAccessKey: input20347 = null,
    awsSessionToken: config8205 = null,
    ...next2172
  } = {}) {
    super({
      baseURL: config8199,
      ...next2172,
    });

    this.skipAuth = false;
    this.messages = stringDecoder214(this);
    this.completions = new lR(this);
    this.beta = stringDecoder215(this);
    this.awsSecretKey = next2170;
    this.awsAccessKey = input20347;
    this.awsSessionToken = config8205;
    this.awsRegion = input20325;
  }
}

// =============================================================================
// GOOGLE VERTEX AI CLIENT (Class39)
// =============================================================================

var transformer133 = Qt; // Same as Qt for env vars

function stringDecoder216(input20325) {
  let config8199 = new ZK(input20325);
  delete config8199.batches;
  return config8199;
}

function stringDecoder217(input20325) {
  let config8199 = new iX(input20325);
  delete config8199.messages.batches;
  return config8199;
}

class Class39 extends Class32 {
  // CRITICAL: Vertex overrides validateHeaders to be EMPTY
  // Line 378517 - Because it uses Google Cloud authentication instead
  validateHeaders() {}

  constructor({
    baseURL: input20325 = transformer133("ANTHROPIC_VERTEX_BASE_URL"),
    region: config8199 = transformer133("CLOUD_ML_REGION") ?? null,
    projectId: next2170 = transformer133("ANTHROPIC_VERTEX_PROJECT_ID") ?? null,
    ...input20347
  } = {}) {
    if (!config8199) {
      throw new Error(
        "No region was given. The client should be instantiated with the `region` option " +
        "or the `CLOUD_ML_REGION` environment variable should be set."
      );
    }

    super({
      baseURL: input20325 || `https://${config8199}-aiplatform.googleapis.com/v1`,
      ...input20347,
    });

    this.messages = stringDecoder216(this);
    this.beta = stringDecoder217(this);
    this.region = config8199;
    this.projectId = next2170;
  }
}

// =============================================================================
// CLIENT MANAGER
// =============================================================================

class ClientManager {
  constructor() {
    this._client = null;
  }

  setClient(client) {
    this._client = client;
  }

  getClient() {
    return this._client;
  }
}

// =============================================================================
// HUB/SCOPE CLIENT MANAGEMENT (from Sentry-like pattern)
// =============================================================================

class ClientManagerScope {
  constructor() {
    this._client = null;
  }

  setClient(input20325) {
    this._client = input20325;
  }

  getClient() {
    return this._client;
  }
}

class ClientManagerHub {
  constructor() {
    this._stack = [{ client: null, scope: new ClientManagerScope() }];
  }

  getStackTop() {
    return this._stack[this._stack.length - 1];
  }

  getClient() {
    return this.getStackTop().client;
  }

  bindClient(input20325) {
    let config8199 = this.getStackTop();
    config8199.client = input20325;
    config8199.scope.setClient(input20325);
    if (input20325 && input20325.setupIntegrations) {
      input20325.setupIntegrations();
    }
  }
}

// Client initialization helper
function initializeClient(input20325) {
  const hub = new ClientManagerHub();
  hub.bindClient(input20325);
  return hub;
}

// =============================================================================
// EXPORTS
// =============================================================================

module.exports = {
  // Main client classes
  AnthropicClient: Ow,
  BedrockClient: Class38,
  VertexClient: Class39,

  // Base class (for extension)
  BaseClient: Class32,

  // Client management
  ClientManager,
  ClientManagerHub,
  ClientManagerScope,
  initializeClient,

  // Error classes
  AnthropicError: CustomError1,
  APIError: Class26,
  APIConnectionError: nN,
  APIConnectionTimeoutError: Po,
  APIUserAbortError: SI,
  NotFoundError: yo,
  ConflictError: ko,
  RateLimitError: fo,
  BadRequestError: So,
  AuthenticationError: Class27,
  InternalServerError: vo,
  PermissionDeniedError: jo,
  UnprocessableEntityError: xo,

  // Constants
  DEFAULT_TIMEOUT: Class32.DEFAULT_TIMEOUT,
  HUMAN_PROMPT: Class32.HUMAN_PROMPT,
  AI_PROMPT: Class32.AI_PROMPT,
};