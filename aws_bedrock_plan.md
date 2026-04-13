Here is Claude's plan:
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Wire AWS Bedrock into Active Client Flow

 Context

 The codebase has AWS Bedrock infrastructure ported from JavaScript (BedrockClient struct, AWS credential providers, SigV4 signer) but none of it is wired into the active client flow. create_client() always creates AIClientAdapter(AnthropicClient).
 Setting CLAUDE_CODE_USE_BEDROCK=1 does nothing. This plan connects the existing Bedrock pieces to make them usable end-to-end.

 What Already Exists (reuse, don't rewrite)

 - src/auth/client.rs:1600-1670 — BedrockClient struct with constructor, services, skip_auth
 - src/auth/aws.rs:262-481 — Full SignatureV4 signer with sign(), canonical request, HMAC key derivation
 - src/auth/aws.rs:16-70 — AwsCredentials, CredentialProvider trait, EnvCredentialProvider
 - src/auth/aws.rs + src/auth/aws_providers/ — Full credential chain (Env, Container, IMDS, SSO, STS, Cognito, WebIdentity)
 - src/auth/client.rs:1614-1630 — BedrockAnthropicClient wrapping AnthropicClient with empty validate_headers()

 Critical Files to Modify

 1. src/ai/mod.rs — Add Provider enum, extend AIConfig, modify create_client() for Bedrock branching
 2. src/ai/client_adapter.rs — Refactor AIClientAdapter to support both AnthropicClient and BedrockClient via inner enum
 3. src/auth/client.rs — Add chat() and chat_stream() to BedrockClient with path transformation + SigV4 signing
 4. src/auth/aws.rs — Fix .expect() in hmac_sign() (line 473) and .unwrap() in path helpers (lines 352-353, 403-404)

 ---
 Step 1: Fix .expect() / .unwrap() in SigV4 signer

 File: src/auth/aws.rs

 Fix three violations of the "no .unwrap()/.expect()" rule:
 - Line 473: HmacSha256::new_from_slice(key).expect(...) → return Result
 - Line 352: Url::parse(...).unwrap_or_else(|_| ... .unwrap()) → use ?
 - Line 404: Same pattern

 This cascades Result return types to get_signing_key(), calculate_signature(), and sign() (which already returns Result, so just needs internal ? propagation).

 ---
 Step 2: Add Provider enum and Bedrock fields to AIConfig

 File: src/ai/mod.rs

 Add before AIConfig struct:
 #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
 pub enum Provider {
     FirstParty,
     Bedrock,
     Vertex,
 }

 Add fields to AIConfig:
 pub provider: Provider,
 pub aws_region: Option<String>,
 pub aws_access_key: Option<String>,
 pub aws_secret_key: Option<String>,
 pub aws_session_token: Option<String>,
 pub skip_bedrock_auth: bool,
 pub bedrock_bearer_token: Option<String>,

 Add provider detection (matching JS getProvider() at cli-jsdef-fixed.js ~93135):
 pub fn determine_provider() -> Provider {
     if std::env::var("CLAUDE_CODE_USE_BEDROCK").is_ok() {
         Provider::Bedrock
     } else if std::env::var("CLAUDE_CODE_USE_VERTEX").is_ok() {
         Provider::Vertex
     } else {
         Provider::FirstParty
     }
 }

 ---
 Step 3: Add chat() and chat_stream() to BedrockClient

 File: src/auth/client.rs (after BedrockClient::new() at line 1670)

 3a. Request transformation method

 Matching JS buildRequest() at cli-jsdef-fixed.js ~293236:
 - Serialize ChatRequest to serde_json::Value
 - Extract and remove model from body
 - Extract and remove stream from body
 - Add anthropic_version: "bedrock-2023-05-31" if not present
 - Build path: /model/{model}/invoke or /model/{model}/invoke-with-response-stream

 3b. SigV4 signing method

 - If skip_auth → return immediately
 - If AWS_BEARER_TOKEN_BEDROCK env var → add Authorization: Bearer {token}, return
 - Otherwise → use existing SignatureV4::new("bedrock") + sign() with stored credentials
 - SigV4 signer already adds x-amz-date, x-amz-security-token, Authorization headers

 3c. chat() method

 - Call transform_request(request, false) → get path + body
 - Build headers: content-type, accept, host (required for SigV4)
 - Call sign_request() to add AWS auth headers
 - POST to {base_url}{path} using self.inner.base.http_client
 - Parse response as ChatResponse

 3d. chat_stream() method

 - Call transform_request(request, true) → get streaming path
 - Same header construction + signing
 - POST request, get bytes_stream()
 - Parse SSE stream using existing parse_sse_stream() (make it pub(crate) if needed)
 - Note: Bedrock with Anthropic's Converse API returns standard SSE when Accept: application/json + streaming path is used. The AWS Event Stream binary format is only for the lower-level InvokeModelWithResponseStream API. Since we're using the Anthropic
 SDK path (which Bedrock proxies), SSE parsing should work.

 ---
 Step 4: Refactor AIClientAdapter to support both providers

 File: src/ai/client_adapter.rs

 4a. Add inner enum

 enum InnerClient {
     Anthropic(Arc<AnthropicClient>),
     Bedrock(Arc<BedrockClient>),
 }

 4b. Add new_bedrock() constructor

 pub fn new_bedrock(config: AIConfig) -> Result<Self> {
     let bedrock = create_bedrock_from_ai_config(&config)?;
     Ok(Self { inner: InnerClient::Bedrock(bedrock), config })
 }

 4c. Update chat() to dispatch on inner enum

 pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
     match &self.inner {
         InnerClient::Anthropic(c) => c.chat(&request).await...,
         InnerClient::Bedrock(c) => c.chat(&request).await...,
     }
 }

 4d. Update chat_stream() — use Box<dyn Stream>

 The two branches return different impl Stream types. Change return type to Box<dyn Stream<Item = Result<StreamEvent>> + Send + Unpin>:
 pub async fn chat_stream(&self, request: ChatRequest)
     -> Result<Box<dyn Stream<Item = Result<StreamEvent>> + Send + Unpin>>
 {
     match &self.inner {
         InnerClient::Anthropic(c) => {
             let s = c.chat_stream(&request).await...;
             Ok(Box::new(Box::pin(s)))
         }
         InnerClient::Bedrock(c) => {
             let s = c.chat_stream(&request).await...;
             Ok(Box::new(Box::pin(s)))
         }
     }
 }

 This changes the return type — all callers currently use impl Stream, which is compatible with Box<dyn Stream> via StreamExt. May need minor adjustments at call sites.

 4e. Add create_bedrock_from_ai_config() helper

 Builds BedrockClient::new() from AIConfig fields (region, credentials, skip_auth). Sets default headers (x-app, user-agent) same as Anthropic path.

 ---
 Step 5: Modify create_client() for provider branching

 File: src/ai/mod.rs (replace lines 384-397)

 Matching JS client creation at cli-jsdef-fixed.js ~350370:

 pub async fn create_client() -> Result<client_adapter::AIClientAdapter> {
     match determine_provider() {
         Provider::Bedrock => create_bedrock_client().await,
         Provider::Vertex => Err(Error::Config("Vertex AI not yet supported".into())),
         Provider::FirstParty => create_first_party_client().await,
     }
 }

 5a. create_first_party_client() — extract existing logic from current create_client()

 5b. create_bedrock_client() — new function:

 - Resolve region: AWS_REGION || AWS_DEFAULT_REGION || "us-east-1"
 - Check CLAUDE_CODE_SKIP_BEDROCK_AUTH and AWS_BEARER_TOKEN_BEDROCK
 - If neither skip flag: resolve credentials via DefaultCredentialProvider::new().get_credentials().await
 - Build AIConfig with provider=Bedrock, credentials, region
 - Call AIClientAdapter::new_bedrock(config)

 5c. Update load_config() validation

 Skip API key requirement when provider == Bedrock (Bedrock uses AWS auth, not API keys).

 ---
 Step 6: Update call sites for chat_stream return type change

 Impact assessment: chat_stream() return type changes from impl Stream<...> to Box<dyn Stream<...> + Send + Unpin>.

 Main call sites:
 - src/tui/state.rs — start_agent_loop() streaming loop
 - src/tui/print_mode.rs — print mode streaming

 Both already use StreamExt methods (.next(), etc.) which work identically on Box<dyn Stream>. The change should be transparent — verify with cargo build.

 ---
 Environment Variables (matching JavaScript)

 ┌───────────────────────────────────────────────────────────────┬─────────────────────────────────┬──────────────────────────────────────────┐
 │                            Env Var                            │             Purpose             │                Checked In                │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ CLAUDE_CODE_USE_BEDROCK                                       │ Enable Bedrock provider         │ determine_provider()                     │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ AWS_REGION / AWS_DEFAULT_REGION                               │ AWS region (default: us-east-1) │ create_bedrock_client()                  │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ ANTHROPIC_BEDROCK_BASE_URL                                    │ Override base URL               │ BedrockClient::new() (already exists)    │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_SESSION_TOKEN │ Credentials via chain           │ EnvCredentialProvider (already exists)   │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ AWS_BEARER_TOKEN_BEDROCK                                      │ Bearer token (skips SigV4)      │ create_bedrock_client() + sign_request() │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ CLAUDE_CODE_SKIP_BEDROCK_AUTH                                 │ Skip all auth (proxy mode)      │ create_bedrock_client()                  │
 ├───────────────────────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────┤
 │ ANTHROPIC_SMALL_FAST_MODEL_AWS_REGION                         │ Per-model region override       │ Future enhancement                       │
 └───────────────────────────────────────────────────────────────┴─────────────────────────────────┴──────────────────────────────────────────┘

 ---
 Verification

 1. cargo build — must compile clean with no warnings
 2. cargo test — all existing 201+ tests must pass
 3. Unit tests for new code:
   - test_determine_provider() — verify env var logic
   - test_bedrock_transform_request() — verify path + body transformation
   - test_bedrock_transform_request_streaming() — verify streaming path
   - test_bedrock_skip_auth() — verify no signing when skip_auth=true
   - test_sigv4_no_expect() — verify error handling in hmac_sign
 4. Manual TUI test (user must run):
   - Set CLAUDE_CODE_USE_BEDROCK=1, AWS_REGION=us-east-1, valid AWS credentials
   - Run cargo run → verify connection to Bedrock endpoint
   - If no Bedrock access: set CLAUDE_CODE_SKIP_BEDROCK_AUTH=1 and ANTHROPIC_BEDROCK_BASE_URL=https://api.anthropic.com/v1 to verify the plumbing without real Bedrock (requests will fail auth but proves the routing works)
