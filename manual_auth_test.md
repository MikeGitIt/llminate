# Manual Authentication Testing Procedures

This document provides step-by-step manual testing procedures for authentication functions that require real credentials and live API interactions.

## Prerequisites

- Working installation of the TUI application (`cargo build --release`)
- Valid Anthropic API key or Claude.ai OAuth credentials
- Access to platform-specific credential stores (Keychain, secret-tool, etc.)

## Test 1: OAuth Token Retrieval (getOAuthToken)

### Purpose
Verify that the `get_oauth_token` function correctly retrieves OAuth tokens from various sources.

### Test Steps

#### 1.1 Environment Variable (JSON Format)
1. Export OAuth token as JSON:
   ```bash
   export CLAUDE_CODE_OAUTH_TOKEN='{"accessToken":"your-token","refreshToken":"refresh","expiresAt":1735689600,"scopes":["user:inference"]}'
   ```
2. Launch TUI: `cargo run`
3. Attempt to send a message that requires authentication
4. Check debug logs for: "Found CLAUDE_CODE_OAUTH_TOKEN environment variable"
5. Verify the request succeeds with Bearer authentication

#### 1.2 Environment Variable (Plain Token)
1. Export OAuth token as plain string:
   ```bash
   export CLAUDE_CODE_OAUTH_TOKEN='sk-ant-api03-...'
   ```
2. Launch TUI: `cargo run`
3. Send a test message
4. Verify authentication succeeds

#### 1.3 Stored Credentials
1. Clear environment variables:
   ```bash
   unset CLAUDE_CODE_OAUTH_TOKEN
   unset ANTHROPIC_AUTH_TOKEN
   ```
2. Use `/login` command in TUI to authenticate via OAuth flow
3. Quit and restart TUI
4. Send a message without re-authenticating
5. Verify stored OAuth credentials are loaded automatically

## Test 2: Scope Validation (hasValidScopes)

### Purpose
Verify that OAuth tokens are validated for required scopes.

### Test Steps

1. Create a test OAuth token with limited scopes:
   ```bash
   export CLAUDE_CODE_OAUTH_TOKEN='{"accessToken":"test","scopes":["user:read"]}'
   ```
2. Launch TUI with debug logging: `RUST_LOG=debug cargo run`
3. Attempt to send a message
4. Verify error message about missing `user:inference` scope
5. Update token with correct scope:
   ```bash
   export CLAUDE_CODE_OAUTH_TOKEN='{"accessToken":"test","scopes":["user:inference"]}'
   ```
6. Restart TUI and verify authentication succeeds

## Test 3: OAuth Access Check (hasOAuthAccess)

### Purpose
Verify the combined check for OAuth availability and validity.

### Test Steps

1. Launch TUI without any authentication
2. Run internal diagnostic (if available) or check logs
3. Verify `has_oauth_access` returns false
4. Authenticate using `/login`
5. Verify `has_oauth_access` now returns true
6. Manually corrupt the stored token (edit `.credentials.json`)
7. Restart TUI and verify `has_oauth_access` returns false

## Test 4: Managed API Key Retrieval (getManagedApiKey)

### macOS Testing

#### 4.1 Store and Retrieve API Key
1. Store API key in Keychain:
   ```bash
   security add-generic-password -a $USER -s "Claude Code-api-key" -w "sk-ant-api03-test-key" -U
   ```
2. Clear all environment variables:
   ```bash
   unset ANTHROPIC_API_KEY
   unset ANTHROPIC_AUTH_TOKEN
   unset CLAUDE_CODE_OAUTH_TOKEN
   ```
3. Launch TUI: `cargo run`
4. Send a test message
5. Check logs for: "Using /login managed key from keychain"
6. Verify authentication succeeds

#### 4.2 Cleanup
```bash
security delete-generic-password -a $USER -s "Claude Code-api-key"
```

### Linux Testing (GNOME Keyring)

#### 4.1 Store and Retrieve API Key
1. Install secret-tool if not present:
   ```bash
   sudo apt-get install libsecret-tools  # Debian/Ubuntu
   sudo dnf install libsecret-tools      # Fedora
   ```
2. Store API key:
   ```bash
   echo -n "sk-ant-api03-test-key" | secret-tool store --label="Claude Code API Key" service "Claude Code-api-key" username "$USER"
   ```
3. Clear environment variables and launch TUI
4. Verify authentication using stored key
5. Check logs for: "Using /login managed key from secret-tool"

#### 4.2 Cleanup
```bash
secret-tool clear service "Claude Code-api-key" username "$USER"
```

### Linux Testing (KDE Wallet)

#### 4.1 Store and Retrieve API Key
1. Ensure KWallet is running and unlocked
2. Store API key:
   ```bash
   echo -n "sk-ant-api03-test-key" | kwallet-query --folder "Claude Code" --write-password "Claude Code-api-key" kdewallet
   ```
3. Clear environment variables and launch TUI
4. Verify authentication using stored key
5. Check logs for: "Using /login managed key from kwallet"

### Windows Testing

#### 4.1 Store and Retrieve API Key
1. Open PowerShell as Administrator
2. Store API key:
   ```powershell
   $secureString = ConvertTo-SecureString 'sk-ant-api03-test-key' -AsPlainText -Force
   $cred = New-Object System.Management.Automation.PSCredential('ClaudeCode', $secureString)
   New-StoredCredential -Target 'Claude Code-api-key' -Credentials $cred -Type Generic -Persist LocalMachine
   ```
3. Clear environment variables and launch TUI
4. Verify authentication using stored key
5. Check logs for: "Using /login managed key from Windows Credential Manager"

#### 4.2 Cleanup
```powershell
Remove-StoredCredential -Target 'Claude Code-api-key' -Type Generic
```

## Test 5: Authentication Priority Chain

### Purpose
Verify that authentication sources are tried in the correct priority order.

### Test Steps

1. Set up multiple authentication sources:
   ```bash
   # Priority 3: API Key
   export ANTHROPIC_API_KEY="priority-3-key"
   
   # Priority 5: Keychain (macOS)
   security add-generic-password -a $USER -s "Claude Code-api-key" -w "priority-5-key" -U
   ```

2. Launch TUI and verify it uses Priority 3 (ANTHROPIC_API_KEY)
3. Now add higher priority:
   ```bash
   export CLAUDE_CODE_OAUTH_TOKEN="priority-2-token"
   ```
4. Restart TUI and verify it uses Priority 2
5. Add highest priority:
   ```bash
   export ANTHROPIC_AUTH_TOKEN="priority-1-token"
   ```
6. Restart TUI and verify it uses Priority 1
7. Clean up all test credentials

## Test 6: OAuth Token Refresh

### Purpose
Verify that expired OAuth tokens are automatically refreshed.

### Prerequisites
- Valid OAuth refresh token
- Access to Anthropic's OAuth refresh endpoint

### Test Steps

1. Obtain valid OAuth credentials via `/login`
2. Manually edit `.credentials.json` to set `expiresAt` to a past timestamp
3. Launch TUI with debug logging
4. Send a message
5. Check logs for: "OAuth token needs refresh"
6. Verify automatic refresh attempt
7. If refresh succeeds, verify new token is saved
8. If refresh fails, verify fallback to re-authentication

## Test 7: API Key Helper Script

### Purpose
Verify that external helper scripts can provide API keys.

### Test Steps

1. Create a helper script:
   ```bash
   cat > ~/api_key_helper.sh << 'EOF'
   #!/bin/bash
   echo "sk-ant-api03-helper-key"
   EOF
   chmod +x ~/api_key_helper.sh
   ```

2. Configure TUI to use helper:
   ```json
   {
     "apiKeyHelper": "~/api_key_helper.sh"
   }
   ```

3. Clear all other authentication sources
4. Launch TUI
5. Verify authentication using helper-provided key
6. Check logs for: "Using apiKeyHelper"

## Test 8: Error Handling

### Purpose
Verify graceful handling of authentication failures.

### Test Steps

1. **Invalid API Key**:
   - Set `ANTHROPIC_API_KEY` to an invalid key
   - Launch TUI and attempt to send a message
   - Verify clear error message about invalid authentication

2. **Expired OAuth Token**:
   - Use an expired OAuth token with no refresh token
   - Verify appropriate error and prompt to re-authenticate

3. **Network Failures**:
   - Disconnect network during OAuth refresh
   - Verify timeout and fallback behavior

4. **Corrupted Storage**:
   - Corrupt `.credentials.json` with invalid JSON
   - Verify TUI handles gracefully and prompts for authentication

## Test 9: Cross-Platform Compatibility

For developers testing on multiple platforms:

1. **macOS → Linux Migration**:
   - Export credentials from macOS Keychain
   - Import to Linux secret-tool
   - Verify seamless authentication

2. **Linux → Windows Migration**:
   - Export from secret-tool
   - Import to Windows Credential Manager
   - Verify authentication works

## Automated Test Execution

Run integration tests (requires appropriate platform):

```bash
# Run all integration tests
cargo test --test test_auth_integration

# Run only on macOS
cargo test --test test_auth_integration --target x86_64-apple-darwin

# Run with verbose output
cargo test --test test_auth_integration -- --nocapture

# Run specific test
cargo test --test test_auth_integration test_macos_keychain_api_key
```

## Troubleshooting

### Common Issues

1. **Keychain Access Denied (macOS)**:
   - Ensure Terminal has Keychain access in System Preferences
   - May need to unlock Keychain first

2. **Secret-tool Not Found (Linux)**:
   - Install: `sudo apt-get install libsecret-tools`
   - Ensure D-Bus session is running

3. **KWallet Locked (Linux)**:
   - Unlock KWallet before testing
   - May need to configure KWallet to allow CLI access

4. **PowerShell Execution Policy (Windows)**:
   - Run: `Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser`

### Debug Logging

Enable detailed logging for troubleshooting:

```bash
RUST_LOG=debug cargo run
RUST_LOG=llminate::auth=trace cargo run  # Auth module only
```

## Reporting Issues

When reporting authentication issues, include:

1. Platform and OS version
2. Authentication method being used
3. Debug logs (with sensitive data redacted)
4. Steps to reproduce
5. Expected vs actual behavior