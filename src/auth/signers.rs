use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::Value;

/// Trait for HTTP request signers
pub trait HttpSigner {
    fn sign(&self, headers: &mut HeaderMap, identity: &Value, signing_properties: Option<&Value>) -> Result<()>;
}

/// HTTP Bearer Token Authentication Signer - matches JavaScript HttpBearerAuthSigner
pub struct HttpBearerAuthSigner;

impl HttpSigner for HttpBearerAuthSigner {
    fn sign(&self, headers: &mut HeaderMap, identity: &Value, _signing_properties: Option<&Value>) -> Result<()> {
        // Get token from identity
        let token = identity.get("token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("request could not be signed with `token` since the `token` is not defined"))?;
        
        // Set Authorization header with Bearer token
        let auth_value = format!("Bearer {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value)
                .map_err(|e| anyhow::anyhow!("Invalid authorization header value: {}", e))?
        );
        
        Ok(())
    }
}

/// HTTP API Key Authentication Signer - matches JavaScript HttpApiKeyAuthSigner
pub struct HttpApiKeyAuthSigner;

impl HttpSigner for HttpApiKeyAuthSigner {
    fn sign(&self, headers: &mut HeaderMap, identity: &Value, signing_properties: Option<&Value>) -> Result<()> {
        let props = signing_properties
            .ok_or_else(|| anyhow::anyhow!("request could not be signed with `apiKey` since the `name` and `in` signer properties are missing"))?;
        
        let name = props.get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow::anyhow!("request could not be signed with `apiKey` since the `name` signer property is missing"))?;
        
        let location = props.get("in")
            .and_then(|l| l.as_str())
            .ok_or_else(|| anyhow::anyhow!("request could not be signed with `apiKey` since the `in` signer property is missing"))?;
        
        let api_key = identity.get("apiKey")
            .and_then(|k| k.as_str())
            .ok_or_else(|| anyhow::anyhow!("request could not be signed with `apiKey` since the `apiKey` is not defined"))?;
        
        if location == "header" {
            // Add to headers
            let value = if let Some(scheme) = props.get("scheme").and_then(|s| s.as_str()) {
                format!("{} {}", scheme, api_key)
            } else {
                api_key.to_string()
            };
            
            use reqwest::header::HeaderName;
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).map_err(|e| anyhow::anyhow!("Invalid header name: {}", e))?,
                HeaderValue::from_str(&value).map_err(|e| anyhow::anyhow!("Invalid header value: {}", e))?
            );
        } else if location == "query" {
            // Query parameters would need to be handled differently
            // For now, return error as we don't handle query params in headers
            return Err(anyhow::anyhow!("Query parameter authentication not yet implemented"));
        } else {
            return Err(anyhow::anyhow!("request can only be signed with `apiKey` locations `query` or `header`, but found: `{}`", location));
        }
        
        Ok(())
    }
}

/// No Authentication Signer - matches JavaScript NoAuthSigner
pub struct NoAuthSigner;

impl HttpSigner for NoAuthSigner {
    fn sign(&self, _headers: &mut HeaderMap, _identity: &Value, _signing_properties: Option<&Value>) -> Result<()> {
        // Pass-through, no authentication
        Ok(())
    }
}