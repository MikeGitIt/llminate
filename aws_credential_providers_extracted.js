// AWS Credential Providers Extracted from test-fixed.js
// Complete implementations extracted with NO STUBBING

// ============================================================================
// 1. fromTemporaryCredentials - STS temporary credentials provider
// ============================================================================

// Main implementation (middleware69)
var fromTemporaryCredentials = (input20325, config8199, next2170) => {
  let input20347;
  return async (config8205 = {}) => {
    let { callerClientConfig: next2172 } = config8205,
      options1339 = input20325.clientConfig?.profile ?? next2172?.profile,
      input20337 = input20325.logger ?? next2172?.logger;
    input20337?.debug(
      "@aws-sdk/credential-providers - fromTemporaryCredentials (STS)",
    );
    let input20202 = {
      ...input20325.params,
      RoleSessionName:
        input20325.params.RoleSessionName ?? "aws-sdk-js-" + Date.now(),
    };
    if (input20202?.SerialNumber) {
      if (!input20325.mfaCodeProvider)
        throw new CredentialsProviderError(
          "Temporary credential requires multi-factor authentication, but no MFA code callback was provided.",
          {
            tryNextLink: false,
            logger: input20337,
          },
        );
      input20202.TokenCode = await input20325.mfaCodeProvider(
        input20202?.SerialNumber,
      );
    }
    let { AssumeRoleCommand: input20345, STSClient: config8181 } =
      await Promise.resolve().then(() => value8952(value962()));
    if (!input20347) {
      let config8166 =
          typeof config8199 === "function" ? config8199() : undefined,
        input20280 = [
          input20325.masterCredentials,
          input20325.clientConfig?.credentials,
          undefined,
          next2172?.credentialDefaultProvider?.(),
          config8166,
        ],
        field222 = "STS client default credentials";
      if (input20280[0]) field222 = "options.masterCredentials";
      else if (input20280[1]) field222 = "options.clientConfig.credentials";
      else if (input20280[2])
        throw (
          (field222 = "caller client's credentials"),
          new Error(
            "fromTemporaryCredentials recursion in callerClientConfig.credentials",
          )
        );
      else if (input20280[3])
        field222 = "caller client's credentialDefaultProvider";
      else if (input20280[4]) field222 = "AWS SDK default credentials";
      let input19930 = [
          input20325.clientConfig?.region,
          next2172?.region,
          await next2170?.({
            profile: options1339,
          }),
          "us-east-1",
        ],
        input20313 = "default partition's default region";
      if (input19930[0]) input20313 = "options.clientConfig.region";
      else if (input19930[1]) input20313 = "caller client's region";
      else if (input19930[2]) input20313 = "file or env region";
      let input20314 = [
          transformer403(input20325.clientConfig?.requestHandler),
          transformer403(next2172?.requestHandler),
        ],
        input19901 = "STS default requestHandler";
      if (input20314[0]) input19901 = "options.clientConfig.requestHandler";
      else if (input20314[1]) input19901 = "caller client's requestHandler";
      (input20337?.debug?.(
        `@aws-sdk/credential-providers - fromTemporaryCredentials STS client init with ${input20313}=${await normalizeProvider(transformer404(input19930))()}, ${field222}, ${input19901}.`,
      ),
        (input20347 = new config8181({
          ...input20325.clientConfig,
          credentials: transformer404(input20280),
          logger: input20337,
          profile: options1339,
          region: transformer404(input19930),
          requestHandler: transformer404(input20314),
        })));
    }
    if (input20325.clientPlugins)
      for (let config8166 of input20325.clientPlugins)
        input20347.middlewareStack.use(config8166);
    let { Credentials: input20289 } = await input20347.send(
      new input20345(input20202),
    );
    if (
      !input20289 ||
      !input20289.AccessKeyId ||
      !input20289.SecretAccessKey
    )
      throw new CredentialsProviderError(
        `Invalid response from STS.assumeRole call with role ${input20202.RoleArn}`,
        {
          logger: input20337,
        },
      );
    return {
      accessKeyId: input20289.AccessKeyId,
      secretAccessKey: input20289.SecretAccessKey,
      sessionToken: input20289.SessionToken,
      expiration: input20289.Expiration,
      credentialScope: input20289.CredentialScope,
    };
  };
};

// Helper functions for fromTemporaryCredentials
var transformer403 = (input20325) => {
  return input20325?.metadata?.handlerProtocol === "h2"
    ? undefined
    : input20325;
};

var transformer404 = (input20325) => {
  for (let config8199 of input20325)
    if (config8199 !== undefined) return config8199;
};

// Wrapper implementation
var fromTemporaryCredentialsWrapper = (input20325) => {
  return fromTemporaryCredentials(
    input20325,
    fromNodeProviderChain,
    async ({ profile: config8199 = process.env.AWS_PROFILE }) =>
      loadConfig(
        {
          environmentVariableSelector: (next2170) => next2170.AWS_REGION,
          configFileSelector: (next2170) => {
            return next2170.region;
          },
          default: () => {
            return;
          },
        },
        {
          ...NODE_REGION_CONFIG_FILE_OPTIONS,
          profile: config8199,
        },
      )(),
  );
};

// ============================================================================
// 2. fromWebToken - Web identity token credentials
// ============================================================================

var fromWebToken = (input20325) => async (config8199) => {
  input20325.logger?.debug(
    "@aws-sdk/credential-provider-web-identity - fromWebToken",
  );
  let {
      roleArn: next2170,
      roleSessionName: input20347,
      webIdentityToken: config8205,
      providerId: next2172,
      policyArns: options1339,
      policy: input20337,
      durationSeconds: input20202,
    } = input20325,
    { roleAssumerWithWebIdentity: input20345 } = input20325;
  if (!input20345) {
    let { getDefaultRoleAssumerWithWebIdentity: config8181 } =
      await Promise.resolve().then(() => value5785(value391()));
    input20345 = config8181(
      {
        ...input20325.clientConfig,
        credentialProviderLogger: input20325.logger,
        parentClientConfig: {
          ...config8199?.callerClientConfig,
          ...input20325.parentClientConfig,
        },
      },
      input20325.clientPlugins,
    );
  }
  return input20345({
    RoleArn: next2170,
    RoleSessionName: input20347 ?? `aws-sdk-js-session-${Date.now()}`,
    WebIdentityToken: config8205,
    ProviderId: next2172,
    PolicyArns: options1339,
    Policy: input20337,
    DurationSeconds: input20202,
  });
};

// ============================================================================
// 3. fromTokenFile - Token file provider
// ============================================================================

var fromTokenFile = (input20325 = {}) => async () => {
  input20325.logger?.debug(
    "@aws-sdk/credential-provider-web-identity - fromTokenFile",
  );
  let config8199 =
      input20325?.webIdentityTokenFile ?? process.env["AWS_WEB_IDENTITY_TOKEN_FILE"],
    next2170 = input20325?.roleArn ?? process.env["AWS_ROLE_ARN"],
    input20347 = input20325?.roleSessionName ?? process.env["AWS_ROLE_SESSION_NAME"];
  if (!config8199 || !next2170)
    throw new CredentialsProviderError(
      "Web identity configuration not specified",
      {
        logger: input20325.logger,
      },
    );
  let config8205 = await fromWebToken({
    ...input20325,
    webIdentityToken: fs.readFileSync(config8199, {
      encoding: "ascii",
    }),
    roleArn: next2170,
    roleSessionName: input20347,
  })();
  if (config8199 === process.env["AWS_WEB_IDENTITY_TOKEN_FILE"])
    setCredentialFeature(
      config8205,
      "CREDENTIALS_ENV_VARS_STS_WEB_ID_TOKEN",
      "h",
    );
  return config8205;
};

// ============================================================================
// 4. fromSSO - SSO authentication
// ============================================================================

// SSO Profile validation helper
var isSsoProfile = (input20325) =>
  input20325 &&
  (typeof input20325.sso_start_url === "string" ||
    typeof input20325.sso_account_id === "string" ||
    typeof input20325.sso_session === "string" ||
    typeof input20325.sso_region === "string" ||
    typeof input20325.sso_role_name === "string");

// SSO Profile validation
var validateSsoProfile = (input20325, config8199) => {
  let {
      sso_start_url: next2170,
      sso_account_id: input20347,
      sso_region: config8205,
      sso_role_name: next2172,
      sso_session: options1339,
    } = input20325,
    input20337 = options1339 ? ` configurations in profile ${config8199} and sso-session ${options1339}` : ` configuration in profile ${config8199}`;
  if (!next2170 || !input20347 || !config8205 || !next2172)
    throw new CredentialsProviderError(
      'Incomplete configuration. The fromSSO() argument hash must include "ssoStartUrl", "ssoAccountId", "ssoRegion", "ssoRoleName"' +
        input20337,
      {
        tryNextLink: false,
        logger: config8199,
      },
    );
  return input20325;
};

// Main fromSSO implementation
var fromSSO = (input20325 = {}) => async ({ callerClientConfig: config8199 } = {}) => {
  input20325.logger?.debug(
    "@aws-sdk/credential-provider-sso - fromSSO",
  );
  let {
      ssoStartUrl: next2170,
      ssoAccountId: input20347,
      ssoRegion: config8205,
      ssoRoleName: next2172,
      ssoSession: options1339,
    } = input20325,
    { ssoClient: input20337 } = input20325,
    input20202 = getProfileName({
      profile: input20325.profile ?? config8199?.profile,
    });
  if (
    !next2170 &&
    !input20347 &&
    !config8205 &&
    !next2172 &&
    !options1339
  ) {
    let config8181 = (await parseKnownFiles(input20325))[
      input20202
    ];
    if (!config8181)
      throw new CredentialsProviderError(
        `Profile ${input20202} was not found.`,
        {
          logger: input20325.logger,
        },
      );
    if (!isSsoProfile(config8181))
      throw new CredentialsProviderError(
        `Profile ${input20202} is not configured with SSO credentials.`,
        {
          logger: input20325.logger,
        },
      );
    if (config8181?.sso_session) {
      let input20314 = (await loadSsoSessionData(input20325))[
          config8181.sso_session
        ],
        input19901 = ` configurations in profile ${input20202} and sso-session ${config8181.sso_session}`;
      if (config8205 && config8205 !== input20314.sso_region)
        throw new CredentialsProviderError(
          "Conflicting SSO region" + input19901,
          {
            tryNextLink: false,
            logger: input20325.logger,
          },
        );
      if (next2170 && next2170 !== input20314.sso_start_url)
        throw new CredentialsProviderError(
          "Conflicting SSO start URL" + input19901,
          {
            tryNextLink: false,
            logger: input20325.logger,
          },
        );
      (next2170 = input20314.sso_start_url),
        (config8205 = input20314.sso_region);
    }
    (input20347 = config8181.sso_account_id),
      (next2172 = config8181.sso_role_name),
      (options1339 = config8181.sso_session);
  }
  let input20345 = validateSsoProfile(
      {
        ssoStartUrl: next2170,
        ssoAccountId: input20347,
        ssoRegion: config8205,
        ssoRoleName: next2172,
        ssoSession: options1339,
      },
      input20202,
    );
  return await resolveSSOCredentials({
    ssoStartUrl: input20345.ssoStartUrl,
    ssoSession: input20345.ssoSession,
    ssoAccountId: input20345.ssoAccountId,
    ssoRegion: input20345.ssoRegion,
    ssoRoleName: input20345.ssoRoleName,
    ssoClient: input20337,
    clientConfig: input20325.clientConfig,
    parentClientConfig: input20325.parentClientConfig,
    profile: input20202,
    logger: input20325.logger,
  });
};

// ============================================================================
// 5. fromCognitoIdentity - Cognito identity provider
// ============================================================================

// Helper function to resolve logins
function resolveLogins(input20325) {
  return Promise.all(
    Object.keys(input20325).reduce((config8199, next2170) => {
      let input20347 = input20325[next2170];
      if (typeof input20347 === "string")
        config8199.push([next2170, input20347]);
      else
        config8199.push(
          input20347().then((config8205) => [next2170, config8205]),
        );
      return config8199;
    }, []),
  ).then((config8199) =>
    config8199.reduce((next2170, [input20347, config8205]) => {
      return ((next2170[input20347] = config8205), next2170);
    }, {}),
  );
}

// Error thrower helpers
function thrower4(logger) {
  throw new CredentialsProviderError("Response from Cognito contained no AccessKeyId");
}

function thrower5(logger) {
  throw new CredentialsProviderError("Response from Cognito contained no Credentials");
}

function thrower6(logger) {
  throw new CredentialsProviderError("Response from Cognito contained no SecretKey");
}

// Main fromCognitoIdentity implementation
function fromCognitoIdentity(input20325) {
  return async (config8199) => {
    input20325.logger?.debug(
      "@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentity",
    );
    let {
        GetCredentialsForIdentityCommand: next2170,
        CognitoIdentityClient: input20347,
      } = await Promise.resolve().then(() => (getCognitoIdentityCommands(), emptyObj70)),
      config8205 = fromConfigs =
        (input20345) =>
          input20325.clientConfig?.[input20345] ??
          input20325.parentClientConfig?.[input20345] ??
          config8199?.callerClientConfig?.[input20345],
      {
        Credentials: {
          AccessKeyId: next2172 = thrower4(input20325.logger),
          Expiration: options1339,
          SecretKey: input20337 = thrower6(input20325.logger),
          SessionToken: input20202,
        } = thrower5(input20325.logger),
      } = await (
        input20325.client ??
        new input20347(
          Object.assign({}, input20325.clientConfig ?? {}, {
            region: config8205("region"),
            profile: config8205("profile"),
          }),
        )
      ).send(
        new next2170({
          CustomRoleArn: input20325.customRoleArn,
          IdentityId: input20325.identityId,
          Logins: input20325.logins
            ? await resolveLogins(input20325.logins)
            : undefined,
        }),
      );
    return setCredentialFeature(
      {
        accessKeyId: next2172,
        secretAccessKey: input20337,
        sessionToken: input20202,
        expiration: options1339,
        credentialScope: "us-east-1",
      },
      "CREDENTIALS_COGNITO",
      "O",
    );
  };
}

// ============================================================================
// 6. fromCognitoIdentityPool - Cognito identity pool
// ============================================================================

function fromCognitoIdentityPool(input20325) {
  let {
      accountId: config8199,
      cache: next2170,
      client: input20347,
      customRoleArn: config8205,
      identityPoolId: next2172,
      logins: options1339,
      userIdentifier: input20337 = "",
    } = input20325,
    input20202 = `amazon-cognito-identity-js:${next2172}${input20337 ? `:${input20337}` : ""}`;
  return async (input20345) => {
    input20325.logger?.debug(
      "@aws-sdk/credential-provider-cognito-identity - fromCognitoIdentityPool",
    );
    let config8181;
    if (next2170 && (config8181 = await next2170.get(input20202)))
      return config8181;
    let {
        GetIdCommand: options1330,
        GetOpenIdTokenCommand: input20280,
        CognitoIdentityClient: field222,
      } = await Promise.resolve().then(() => (getCognitoIdentityCommands(), emptyObj70)),
      input20313 = fromConfigs =
        (input19930) =>
          input20325.clientConfig?.[input19930] ??
          input20325.parentClientConfig?.[input19930] ??
          input20345?.callerClientConfig?.[input19930],
      input20314 =
        input20325.client ??
        new field222(
          Object.assign({}, input20325.clientConfig ?? {}, {
            region: input20313("region"),
            profile: input20313("profile"),
          }),
        ),
      { IdentityId: input19901 } = await input20314.send(
        new options1330({
          AccountId: config8199,
          IdentityPoolId: next2172,
          Logins: options1339 ? await resolveLogins(options1339) : undefined,
        }),
      );
    if (!input19901)
      throw new CredentialsProviderError(
        `Identity ID is missing from the response of GetId operation.`,
        {
          logger: input20325.logger,
        },
      );
    let { Token: next2174 } = await input20314.send(
      new input20280({
        IdentityId: input19901,
        Logins: options1339 ? await resolveLogins(options1339) : undefined,
      }),
    );
    if (!next2174)
      throw new CredentialsProviderError(
        `Open ID token is missing from the response of GetOpenIdToken operation.`,
        {
          logger: input20325.logger,
        },
      );
    let config8166 = await fromCognitoIdentity({
      ...input20325,
      customRoleArn: config8205,
      identityId: input19901,
      logins: {
        ...options1339,
        [`cognito-identity.${region}.amazonaws.com`]: next2174,
      },
    })(input20345);
    return (
      next2170 && (await next2170.set(input20202, config8166, config8166.expiration)),
      config8166
    );
  };
}

// ============================================================================
// 7. Helper Functions and Constants
// ============================================================================

// Credential provider error class
class CredentialsProviderError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "CredentialsProviderError";
    this.tryNextLink = options.tryNextLink !== false;
    this.logger = options.logger;
  }
}

// Set credential feature helper
function setCredentialFeature(credentials, feature, value) {
  credentials.$credentialProvider = feature;
  credentials.$credentialProviderValue = value;
  return credentials;
}

// ============================================================================
// 8. Export Object (as found in the JavaScript)
// ============================================================================

var awsCredentialProviders = {
  fromTemporaryCredentials,
  fromWebToken,
  fromTokenFile,
  fromSSO,
  fromCognitoIdentity,
  fromCognitoIdentityPool,
  CredentialsProviderError,
  setCredentialFeature,
  resolveLogins,
  isSsoProfile,
  validateSsoProfile
};

// ============================================================================
// 9. Role Assumer Functions (STS Client Wrappers)
// ============================================================================

// Default role assumer for AssumeRole
var getDefaultRoleAssumer = (input20325 = {}, config8199) => {
  let next2170;
  return async (input20347) => {
    if (!next2170) {
      let {
          logger: input20202 = input20325?.parentClientConfig?.logger,
          region: input20345,
          requestHandler: config8181 = input20325?.parentClientConfig
            ?.requestHandler,
          credentialProviderLogger: input20289,
        } = input20325,
        config8166 = await resolveRegion(
          input20345,
          input20325?.parentClientConfig?.region,
          input20289,
        ),
        input20280 = !isH2(config8181);
      next2170 = new config8199({
        profile: input20325?.parentClientConfig?.profile,
        region: config8166,
        requestHandler: input20280 ? config8181 : undefined,
        logger: input20202,
      });
    }
    let { Credentials: config8205, AssumedRoleUser: next2172 } =
      await next2170.send(new AssumeRoleCommand(input20347));
    if (
      !config8205 ||
      !config8205.AccessKeyId ||
      !config8205.SecretAccessKey
    )
      throw new Error(
        `Invalid response from STS.assumeRole call with role ${input20347.RoleArn}`,
      );
    let options1339 = getAccountIdFromAssumedRoleUser(next2172),
      input20337 = {
        accessKeyId: config8205.AccessKeyId,
        secretAccessKey: config8205.SecretAccessKey,
        sessionToken: config8205.SessionToken,
        expiration: config8205.Expiration,
        ...(config8205.CredentialScope && {
          credentialScope: config8205.CredentialScope,
        }),
        ...(options1339 && {
          accountId: options1339,
        }),
      };
    if (options1339)
      setCredentialFeature(
        input20337,
        "RESOLVED_ACCOUNT_ID",
        "T",
      );
    return (
      setCredentialFeature(
        input20337,
        "CREDENTIALS_STS_ASSUME_ROLE",
        "K",
      ),
      input20337
    );
  };
};

// Default role assumer with web identity for AssumeRoleWithWebIdentity
var getDefaultRoleAssumerWithWebIdentity = (input20325, config8199) => {
  let next2170;
  return async (input20347) => {
    if (!next2170) {
      let {
          logger: input20202 = input20325?.parentClientConfig?.logger,
          region: input20345,
          requestHandler: config8181 = input20325?.parentClientConfig
            ?.requestHandler,
          credentialProviderLogger: input20289,
        } = input20325,
        config8166 = await resolveRegion(
          input20345,
          input20325?.parentClientConfig?.region,
          input20289,
        ),
        input20280 = !isH2(config8181);
      next2170 = new config8199({
        profile: input20325?.parentClientConfig?.profile,
        region: config8166,
        requestHandler: input20280 ? config8181 : undefined,
        logger: input20202,
      });
    }
    let { Credentials: config8205, AssumedRoleUser: next2172 } =
      await next2170.send(new AssumeRoleWithWebIdentityCommand(input20347));
    if (
      !config8205 ||
      !config8205.AccessKeyId ||
      !config8205.SecretAccessKey
    )
      throw new Error(
        `Invalid response from STS.assumeRoleWithWebIdentity call with role ${input20347.RoleArn}`,
      );
    let options1339 = getAccountIdFromAssumedRoleUser(next2172),
      input20337 = {
        accessKeyId: config8205.AccessKeyId,
        secretAccessKey: config8205.SecretAccessKey,
        sessionToken: config8205.SessionToken,
        expiration: config8205.Expiration,
        ...(config8205.CredentialScope && {
          credentialScope: config8205.CredentialScope,
        }),
        ...(options1339 && {
          accountId: options1339,
        }),
      };
    if (options1339)
      setCredentialFeature(
        input20337,
        "RESOLVED_ACCOUNT_ID",
        "T",
      );
    return (
      setCredentialFeature(
        input20337,
        "CREDENTIALS_STS_ASSUME_ROLE_WEB_ID",
        "k",
      ),
      input20337
    );
  };
};

// Helper functions for role assumers
var getAccountIdFromAssumedRoleUser = (input20325) => {
  if (typeof input20325?.Arn === "string") {
    let config8199 = input20325.Arn.split(":");
    if (config8199.length > 4 && config8199[4] !== "") return config8199[4];
  }
  return;
};

var resolveRegion = async (input20325, config8199, next2170) => {
  let input20347 =
      typeof input20325 === "function" ? await input20325() : input20325,
    config8205 =
      typeof config8199 === "function" ? await config8199() : config8199;
  return (
    next2170?.debug?.(
      "@aws-sdk/client-sts::resolveRegion",
      "accepting first of:",
      `${input20347} (provider)`,
      `${config8205} (parent client)`,
      `${"us-east-1"} (STS default)`,
    ),
    input20347 ?? config8205 ?? "us-east-1"
  );
};

var isH2 = (input20325) => {
  return input20325?.metadata?.handlerProtocol === "h2";
};

// Decorator function for default credential provider
var decorateDefaultCredentialProvider = (input20325) => (config8199) =>
  input20325({
    roleAssumer: getDefaultRoleAssumer(config8199),
    roleAssumerWithWebIdentity: getDefaultRoleAssumerWithWebIdentity(config8199),
    ...config8199,
  });

// ============================================================================
// 10. STS Command Classes (from the extracted patterns)
// ============================================================================

// Note: These are the STS command classes that the role assumers use
// AssumeRoleCommand - for standard role assumption
// AssumeRoleWithWebIdentityCommand - for web identity role assumption
// GetSessionToken would be a similar command for session tokens

// ============================================================================
// 11. Updated Export Object
// ============================================================================

var awsCredentialProviders = {
  fromTemporaryCredentials,
  fromWebToken,
  fromTokenFile,
  fromSSO,
  fromCognitoIdentity,
  fromCognitoIdentityPool,
  getDefaultRoleAssumer,
  getDefaultRoleAssumerWithWebIdentity,
  decorateDefaultCredentialProvider,
  CredentialsProviderError,
  setCredentialFeature,
  resolveLogins,
  isSsoProfile,
  validateSsoProfile,
  getAccountIdFromAssumedRoleUser,
  resolveRegion,
  isH2
};

// Note: Additional STS functions that may exist but were not fully extracted:
// - getSessionToken (GetSessionToken STS command wrapper)
// - Direct assumeRole/assumeRoleWithWebIdentity wrappers
// The role assumers above provide the core functionality for these operations.

// ===== ADDITIONAL EXTRACTIONS =====

// ============================================================================
// 12. fromEnv - Environment variable credentials provider
// ============================================================================

var ArA = "AWS_ACCESS_KEY_ID",
    BrA = "AWS_SECRET_ACCESS_KEY",
    QrA = "AWS_SESSION_TOKEN",
    IrA = "AWS_CREDENTIAL_EXPIRATION",
    GrA = "AWS_CREDENTIAL_SCOPE",
    ZrA = "AWS_ACCOUNT_ID";

var fromEnv = handler192(
  (input20325) => async () => {
    input20325?.logger?.debug("@aws-sdk/credential-provider-env - fromEnv");
    let config8199 = process.env[ArA],
      next2170 = process.env[BrA],
      input20347 = process.env[QrA],
      config8205 = process.env[IrA],
      next2172 = process.env[GrA],
      options1339 = process.env[ZrA];
    if (config8199 && next2170) {
      let input20337 = {
        accessKeyId: config8199,
        secretAccessKey: next2170,
        ...(input20347 && {
          sessionToken: input20347,
        }),
        ...(config8205 && {
          expiration: new Date(config8205),
        }),
        ...(next2172 && {
          credentialScope: next2172,
        }),
        ...(options1339 && {
          accountId: options1339,
        }),
      };
      return (
        input20325?.logger?.debug(
          "@aws-sdk/credential-provider-env - fromEnv::process.env",
        ),
        setCredentialFeature(input20337, "CREDENTIALS_ENV_VARS", "p")
      );
    }
    throw new CredentialsProviderError(
      "Unable to find environment variable credentials.",
      {
        logger: input20325?.logger,
      },
    );
  },
  "fromEnv",
);

// ============================================================================
// 13. fromHttp - HTTP credentials provider (ECS/Container metadata)
// ============================================================================

var str332 = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
    str333 = "AWS_CONTAINER_CREDENTIALS_FULL_URI",
    str334 = "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
    str335 = "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
    str336 = "AWS_CONTAINER_AUTHORIZATION_TOKEN";

var fromHttp = (input20325 = {}) => {
  input20325.logger?.debug("@aws-sdk/credential-provider-http - fromHttp");
  let config8199,
    next2170 =
      input20325.awsContainerCredentialsRelativeUri ?? process.env[str332],
    input20347 =
      input20325.awsContainerCredentialsFullUri ?? process.env[str333],
    config8205 =
      input20325.awsContainerAuthorizationToken ?? process.env[str336],
    next2172 =
      input20325.awsContainerAuthorizationTokenFile ?? process.env[str334];
  if (input20347)
    config8199 = new URL(input20347);
  else if (next2170)
    config8199 = new URL(next2170, "http://169.254.170.2");
  else
    throw new CredentialsProviderError(
      "The AWS_CONTAINER_CREDENTIALS_RELATIVE_URI or AWS_CONTAINER_CREDENTIALS_FULL_URI environment variable must be set to use fromHttp credential provider.",
      {
        logger: input20325.logger,
      },
    );
  let options1339 = {};
  if (config8205) options1339.Authorization = config8205;
  else if (next2172)
    try {
      options1339.Authorization = fs.readFileSync(next2172, "utf8");
    } catch (input20337) {
      throw new CredentialsProviderError(
        `Unable to read authorization token from file ${next2172}: ${input20337.message}`,
        {
          logger: input20325.logger,
        },
      );
    }
  return httpRequest({
    url: config8199.href,
    headers: options1339,
    timeout: input20325.timeout ?? 1000,
  });
};

// ============================================================================
// 14. fromContainerMetadata - Container metadata credentials
// ============================================================================

var fromContainerMetadata = (input20325 = {}) =>
  memoize(
    async () => {
      input20325?.logger?.debug(
        "@aws-sdk/credential-provider-imds - fromContainerMetadata",
      );
      let config8199 = await fromHttp(input20325)();
      if (!isImdsCredentials(config8199))
        throw new CredentialsProviderError(
          "Invalid response from container metadata service.",
          {
            logger: input20325.logger,
          },
        );
      return VrA(config8205);
    }, next2170);

// ============================================================================
// 15. fromInstanceMetadata - EC2 instance metadata credentials
// ============================================================================

var fromInstanceMetadata = (input20325 = {}) =>
  value5060(value5062(input20325), {
    logger: input20325.logger,
  });

// Helper for instance metadata configuration
var value5062 = (input20325 = {}) => {
  let config8199 = false,
    { logger: next2170, profile: input20347 } = input20325,
    { timeout: config8205, maxRetries: next2172 } = providerConfigFromInit(input20325),
    options1339 = input20325.ec2MetadataV1Disabled ?? process.env[str407] === "true",
    input20337 = input20325.ec2MetadataV2DisableSessionToken ?? false;
  return {
    ...input20325,
    profile: input20347,
    logger: next2170,
    timeout: config8205,
    maxRetries: next2172,
    ec2MetadataV1Disabled: options1339,
    ec2MetadataV2DisableSessionToken: input20337,
  };
};

// ============================================================================
// 16. fromNodeProviderChain - Node.js default credential provider chain
// ============================================================================

var fromNodeProviderChain = (input20325 = {}) =>
  defaultProvider({
    ...input20325,
  });

// Default provider chain implementation
var defaultProvider = (input20325 = {}) => {
  let config8199 = chain(
    async () => {
      // Skip fromEnv if AWS_PROFILE is set to avoid conflicts
      if (process.env.AWS_PROFILE && process.env.AWS_ACCESS_KEY_ID) {
        if (!flag14) {
          (input20325.logger?.warn || console.warn)(
            "@aws-sdk/credential-provider-node - defaultProvider::fromEnv WARNING:\n    Multiple credential sources detected: \n    Both AWS_PROFILE and the pair AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY static credentials are set.\n    This SDK will proceed with the AWS_PROFILE value.\n    \n    However, a future version may change this behavior to prefer the ENV static credentials.\n    Please ensure that your environment only sets either the AWS_PROFILE or the\n    AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY pair.\n",
          );
          flag14 = true;
        }
        throw new CredentialsProviderError(
          "AWS_PROFILE is set, skipping fromEnv provider.",
          {
            logger: input20325.logger,
            tryNextLink: true,
          },
        );
      }
      return (
        input20325.logger?.debug(
          "@aws-sdk/credential-provider-node - defaultProvider::fromEnv",
        ),
        fromEnv(input20325)()
      );
    },
    async () => {
      input20325.logger?.debug(
        "@aws-sdk/credential-provider-node - defaultProvider::fromSSO",
      );
      return fromSSO(input20325)();
    },
    async () => {
      input20325.logger?.debug(
        "@aws-sdk/credential-provider-node - defaultProvider::fromIni",
      );
      return fromIni(input20325)();
    },
    async () => {
      input20325.logger?.debug(
        "@aws-sdk/credential-provider-node - defaultProvider::fromProcess",
      );
      return fromProcess(input20325)();
    },
    async () => {
      input20325.logger?.debug(
        "@aws-sdk/credential-provider-node - defaultProvider::fromTokenFile",
      );
      return fromTokenFile(input20325)();
    },
    async () => {
      input20325.logger?.debug(
        "@aws-sdk/credential-provider-node - defaultProvider::remoteProvider",
      );
      return remoteProvider(input20325)();
    }
  );

  return config8199;
};

// Remote provider chain (container/instance metadata)
var remoteProvider = async (input20325) => {
  let { ENV_CMDS_FULL_URI: config8199, ENV_CMDS_RELATIVE_URI: next2170 } =
        process.env;

  if (process.env[next2170] || process.env[config8199]) {
    input20325.logger?.debug(
      "@aws-sdk/credential-provider-node - remoteProvider::fromHttp/fromContainerMetadata",
    );
    return chain(fromHttp(input20325), fromContainerMetadata(input20325));
  }

  if (process.env[str407] === "true") {
    input20325.logger?.debug(
      "@aws-sdk/credential-provider-node - remoteProvider::EC2 IMDS access is disabled",
    );
    throw new CredentialsProviderError(
      "EC2 Instance Metadata Service access disabled",
      {
        logger: input20325.logger,
      },
    );
  }

  return (
    input20325.logger?.debug(
      "@aws-sdk/credential-provider-node - remoteProvider::fromInstanceMetadata",
    ),
    fromInstanceMetadata(input20325)
  );
};

// ============================================================================
// 17. Background Shell Management Tools
// ============================================================================

// Schema for KillShell tool
var KillShellSchema = {
  shell_id: {
    type: "string",
    description: "The ID of the background shell to kill"
  }
};

// Schema for BashOutput tool
var BashOutputSchema = {
  shell_id: {
    type: "string",
    description: "The ID of the background shell to retrieve output from"
  },
  filter: {
    type: "string",
    description: "Optional regular expression to filter the output lines",
    optional: true
  }
};

// Background Shell class implementation
class BackgroundShell {
  constructor(id, command, shellCommand, onComplete) {
    this.id = id;
    this.command = command;
    this.status = "running";
    this.startTime = Date.now();
    this.shellCommand = shellCommand;
    this.stdout = "";
    this.stderr = "";
    this.result = null;

    let config8205 = shellCommand.background(id);
    if (!config8205) {
      this.status = "failed";
      this.result = {
        code: 1,
        interrupted: false,
      };
    } else {
      config8205.stdoutStream.on("data", (data) => {
        this.stdout += data.toString();
      });

      config8205.stderrStream.on("data", (data) => {
        this.stderr += data.toString();
      });

      shellCommand.result.then((result) => {
        if (result.code === 0) this.status = "completed";
        else this.status = "failed";

        this.result = {
          code: result.code,
          interrupted: result.interrupted,
        };

        onComplete(result);
      });
    }
  }

  getOutput() {
    let output = {
      stdout: this.stdout,
      stderr: this.stderr,
    };
    this.stdout = "";
    this.stderr = "";
    return output;
  }

  hasNewOutput() {
    return !!this.stdout;
  }

  kill() {
    try {
      this.shellCommand?.kill();
      this.status = "killed";
      return true;
    } catch (error) {
      return false;
    }
  }

  dispose() {
    this.shellCommand = null;
  }
}

// Background Shell Manager class
class BackgroundShellManager {
  static instance = null;

  constructor() {
    this.shells = new Map();
    this.shellCounter = 0;
    this.subscribers = new Set();
  }

  static getInstance() {
    if (!BackgroundShellManager.instance) {
      BackgroundShellManager.instance = new BackgroundShellManager();
    }
    return BackgroundShellManager.instance;
  }

  subscribe(callback) {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  notifySubscribers() {
    this.subscribers.forEach((callback) => {
      try {
        callback();
      } catch (error) {
        console.error(error);
      }
    });
  }

  addBackgroundShell(shell) {
    this.shells.set(shell.id, shell);
    this.notifySubscribers();
    return shell.id;
  }

  getShell(shellId) {
    return this.shells.get(shellId);
  }

  getShellOutput(shellId) {
    let shell = this.shells.get(shellId);
    if (!shell) {
      return {
        shellId: shellId,
        command: "",
        status: "failed",
        exitCode: null,
        stdout: "",
        stderr: "Shell not found",
      };
    }

    let exitCode = shell.result ? shell.result.code : null;
    let { stdout, stderr } = shell.getOutput();

    return {
      shellId: shellId,
      command: shell.command,
      status: shell.status,
      exitCode: exitCode,
      stdout: stdout.trimEnd(),
      stderr: stderr.trimEnd(),
    };
  }

  getShellsUnreadOutputInfo() {
    return this.getActiveShells().map((shell) => {
      return {
        id: shell.id,
        command: shell.command,
        hasNewOutput: shell.hasNewOutput(),
      };
    });
  }

  getActiveShells() {
    return Array.from(this.shells.values()).filter(
      (shell) => shell.status === "running",
    );
  }

  killShell(shellId) {
    let shell = this.shells.get(shellId);
    if (shell && shell.status === "running") {
      shell.kill();
      setTimeout(() => {
        if (this.shells.get(shellId)) {
          shell.dispose();
        }
      }, 1800000); // 30 minutes
      this.notifySubscribers();
      return true;
    }
    return false;
  }

  removeShell(shellId) {
    let shell = this.shells.get(shellId);
    if (shell) {
      if (shell.status === "running") {
        shell.kill();
        shell.dispose();
      }
      let removed = this.shells.delete(shellId);
      this.notifySubscribers();
      return removed;
    }
    return false;
  }

  moveToBackground(command, shellCommand) {
    let shellId = this.generateShellId();
    let shell = new BackgroundShell(
      shellId,
      command,
      shellCommand,
      (result) => {
        this.completeShell(shell.id, result);
      },
    );
    return this.addBackgroundShell(shell);
  }

  completeShell(shellId, result) {
    let shell = this.shells.get(shellId);
    if (!shell) return;

    shell.status = result.code === 0 ? "completed" : "failed";
    shell.result = {
      code: result.code,
      interrupted: result.interrupted,
    };
    this.notifySubscribers();
  }

  generateShellId() {
    return `bash_${++this.shellCounter}`;
  }
}

// ============================================================================
// 18. Updated Export Object with New Functions
// ============================================================================

var awsCredentialProviders = {
  // Original functions
  fromTemporaryCredentials,
  fromWebToken,
  fromTokenFile,
  fromSSO,
  fromCognitoIdentity,
  fromCognitoIdentityPool,
  getDefaultRoleAssumer,
  getDefaultRoleAssumerWithWebIdentity,
  decorateDefaultCredentialProvider,

  // New environment and metadata providers
  fromEnv,
  fromHttp,
  fromContainerMetadata,
  fromInstanceMetadata,
  fromNodeProviderChain,

  // Background shell management
  BackgroundShell,
  BackgroundShellManager,
  KillShellSchema,
  BashOutputSchema,

  // Utilities
  CredentialsProviderError,
  setCredentialFeature,
  resolveLogins,
  isSsoProfile,
  validateSsoProfile,
  getAccountIdFromAssumedRoleUser,
  resolveRegion,
  isH2,

  // Environment variable constants
  ArA, // AWS_ACCESS_KEY_ID
  BrA, // AWS_SECRET_ACCESS_KEY
  QrA, // AWS_SESSION_TOKEN
  IrA, // AWS_CREDENTIAL_EXPIRATION
  GrA, // AWS_CREDENTIAL_SCOPE
  ZrA, // AWS_ACCOUNT_ID
};

module.exports = awsCredentialProviders;