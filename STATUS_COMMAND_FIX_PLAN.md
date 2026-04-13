# Implementation Plan: Fix `/status` Command

## Overview
The `/status` command in the Rust implementation currently displays hardcoded/placeholder data instead of fetching real information from APIs like the JavaScript version does.

## Current State Analysis

### Working Features
- ✅ Version display (from env!)
- ✅ Session ID display
- ✅ Current working directory
- ✅ Tab navigation (Status, Config, Usage)
- ✅ Basic UI layout and styling

### Broken/Missing Features
- ❌ API authentication status (shows "Not authenticated" hardcoded)
- ❌ Organization info (hardcoded as "Not available")
- ❌ Email info (hardcoded as "Not available")
- ❌ Weekly usage statistics (shows placeholder text)
- ❌ Model-specific usage data (not fetched from API)
- ❌ MCP server status (not implemented)
- ❌ Interactive config editing (read-only)
- ❌ Real permissions data

## Implementation Steps

### Phase 1: API Integration Foundation
**Priority: HIGH | Estimated: 2-3 days**

1. **Create Usage API Module** (`src/api/usage.rs`)
   - Define structs for API responses:
     ```rust
     struct UsageStats {
         weekly_total: u64,
         weekly_by_model: HashMap<String, u64>,
         daily_breakdown: Vec<DailyUsage>,
         extra_usage_enabled: bool,
         extra_usage_remaining: Option<u64>,
     }
     ```
   - Implement API client methods:
     - `fetch_usage_stats()` - GET /api/usage/stats
     - `fetch_account_info()` - GET /api/account/info
     - `fetch_organization_info()` - GET /api/organization/info

2. **Update OAuth Module** (`src/oauth.rs`)
   - Add method to check authentication status
   - Add method to get current user info from token
   - Ensure token refresh logic is working

### Phase 2: Status Tab Implementation
**Priority: HIGH | Estimated: 1-2 days**

1. **Update `get_account_info()` function** (`src/tui/interactive_mode.rs`)
   - Replace hardcoded values with actual API calls
   - Cache results for performance (5-minute TTL)
   - Handle API errors gracefully

2. **Add MCP Server Status**
   - Integrate with existing MCP module (`src/mcp.rs`)
   - Display connected servers and their status
   - Show available tools from each server

3. **Add Permissions Display**
   - Fetch actual permissions from API
   - Display file system access, network permissions, etc.

### Phase 3: Usage Tab Implementation  
**Priority: HIGH | Estimated: 2-3 days**

1. **Replace Placeholder Text**
   - Remove "Weekly usage data requires API integration" messages
   - Implement actual API calls in the Usage tab rendering

2. **Implement Usage Bar Rendering**
   - Create accurate progress bars based on real data
   - Show percentage and absolute values
   - Color-code based on usage levels (green/yellow/red)

3. **Add Cost Breakdown**
   - Calculate estimated costs based on usage
   - Display per-model breakdown
   - Show daily/weekly trends

### Phase 4: Config Tab Interactivity
**Priority: MEDIUM | Estimated: 2-3 days**

1. **Make Config Items Editable**
   - Add state management for editing mode
   - Implement toggle functionality for boolean settings
   - Add text input for string settings

2. **Persist Config Changes**
   - Update settings files on change
   - Validate settings before saving
   - Show confirmation messages

3. **Add Config Categories**
   - Group related settings
   - Add descriptions for each setting
   - Implement search/filter functionality

### Phase 5: Error Handling & Polish
**Priority: MEDIUM | Estimated: 1-2 days**

1. **Add Loading States**
   - Show spinners while fetching data
   - Implement skeleton screens for better UX

2. **Error Handling**
   - Display user-friendly error messages
   - Add retry logic for failed API calls
   - Fallback to cached data when offline

3. **Performance Optimization**
   - Implement request debouncing
   - Add response caching layer
   - Optimize re-renders

## File Changes Required

### New Files to Create
- `src/api/usage.rs` - Usage statistics API client
- `src/api/account.rs` - Account information API client
- `src/cache/mod.rs` - Simple caching layer for API responses

### Files to Modify
- `src/tui/interactive_mode.rs` - Update status view rendering
- `src/tui/state.rs` - Add new state fields for API data
- `src/oauth.rs` - Enhance authentication status checking
- `src/config.rs` - Add config persistence methods
- `src/main.rs` - Initialize API clients
- `Cargo.toml` - Add dependencies (if needed)

## Testing Plan

### Unit Tests
- API response parsing
- Cache invalidation logic
- Config validation

### Integration Tests
- Full `/status` command flow
- Tab navigation with real data
- Config persistence

### Manual Testing
- Test with different authentication states
- Verify all three tabs display correct data
- Test error scenarios (network issues, API failures)

## Dependencies

### External Crates (if not already included)
- `reqwest` - For HTTP requests (likely already present)
- `serde_json` - For JSON parsing (already present)
- `chrono` - For date/time handling (likely already present)
- `cached` - For simple caching (optional)

## Success Criteria

1. `/status` command shows real, up-to-date information
2. All three tabs (Status, Config, Usage) work as in JavaScript version
3. No hardcoded placeholder text remains
4. API errors are handled gracefully
5. Performance is acceptable (< 500ms to display initial data)

## Risk Mitigation

### API Changes
- Version the API client
- Add response validation
- Monitor for deprecation warnings

### Authentication Issues  
- Implement token refresh before expiry
- Add clear error messages for auth failures
- Provide fallback to API key auth where possible

### Performance Concerns
- Implement progressive loading
- Cache frequently accessed data
- Add pagination for large datasets

## Timeline Estimate

**Total: 8-13 days**

- Phase 1: 2-3 days
- Phase 2: 1-2 days  
- Phase 3: 2-3 days
- Phase 4: 2-3 days
- Phase 5: 1-2 days

## Notes

- The JavaScript implementation in `cli-jsdef-fixed.js` should be used as the reference for expected behavior
- Consider extracting common API logic into a shared module to avoid duplication
- Ensure backward compatibility with existing settings files
- Add feature flags to gradually roll out changes if needed