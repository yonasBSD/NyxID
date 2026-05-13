//! AWS Signature Version 4 (SigV4) request signing.
//!
//! Spec: <https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html>.
//!
//! This implementation targets the small set of services NyxID actually proxies
//! (currently AWS Cost Explorer per NyxID#716). It assumes:
//! - Non-streaming body — full payload bytes are hashed in one shot.
//! - No `Transfer-Encoding: chunked`.
//! - Caller supplies the final URL + headers; this module returns only the
//!   *additional* headers required to make the request signed.
//!
//! Callers should not depend on header ordering of the returned vec — they're
//! independent name/value pairs to append to the outgoing request.

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{CloudAuthError, CloudAuthResult};

type HmacSha256 = Hmac<Sha256>;

/// Decoded `aws_sigv4` credential payload.
///
/// Stored on `UserApiKey.credential_encrypted` as a JSON object. The
/// `region` and `service` fields are required and must match the target
/// endpoint — for AWS Cost Explorer that's `us-east-1` + `ce`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub service: String,
    /// Optional STS session token. When present, `X-Amz-Security-Token` is
    /// added to the request and included in the signed headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

impl AwsCredentials {
    pub fn from_json(raw: &str) -> CloudAuthResult<Self> {
        let trimmed = raw.trim();
        let creds: Self = serde_json::from_str(trimmed).map_err(|e| {
            CloudAuthError::InvalidCredential(format!(
                "aws_sigv4 credential must be a JSON object with access_key_id, secret_access_key, region, service: {}",
                e
            ))
        })?;
        if creds.access_key_id.is_empty()
            || creds.secret_access_key.is_empty()
            || creds.region.is_empty()
            || creds.service.is_empty()
        {
            return Err(CloudAuthError::InvalidCredential(
                "aws_sigv4 credential is missing one of access_key_id, secret_access_key, region, service".to_string(),
            ));
        }
        Ok(creds)
    }
}

/// A header name/value pair to attach to the outgoing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedHeader {
    pub name: String,
    pub value: String,
}

/// Build the headers required to make `request` a valid SigV4-signed request.
///
/// The caller must:
/// 1. Have already finalized the URL (including any query string).
/// 2. Provide the *exact* set of headers that will be sent (other than the
///    ones this function appends). All headers passed in here are signed,
///    so don't pass headers reqwest will strip or replace.
/// 3. Provide the *exact* body bytes that will be sent. SigV4 hashes them.
///
/// Returns:
/// - `Authorization` (the signature)
/// - `X-Amz-Date` (the ISO 8601 basic-format timestamp)
/// - `X-Amz-Content-Sha256` (the body hash; required for `s3` service, harmless for others)
/// - `X-Amz-Security-Token` (only when `creds.session_token` is set)
///
/// `Host` is computed from `url.host()` and added to the canonical request
/// but is NOT returned — reqwest sets `Host` from the URL automatically and
/// duplicating it would corrupt the signature.
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    creds: &AwsCredentials,
) -> CloudAuthResult<Vec<SignedHeader>> {
    let parsed = Url::parse(url)
        .map_err(|e| CloudAuthError::Signing(format!("invalid URL '{}': {}", url, e)))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| CloudAuthError::Signing(format!("URL '{}' has no host", url)))?;

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    let payload_hash = hex_sha256(body);

    // Build the canonical headers. Lowercased name, trimmed value,
    // sorted ASCII-ascending by name. We always include `host`,
    // `x-amz-date`, and `x-amz-content-sha256`. Caller-supplied headers
    // are folded in; if any conflict on (lowercase) name we keep the
    // caller's value but normalize the name to lowercase.
    let mut canonical: Vec<(String, String)> = Vec::with_capacity(headers.len() + 4);
    canonical.push(("host".to_string(), host_with_port(&parsed, host)));
    canonical.push(("x-amz-date".to_string(), amz_date.clone()));
    canonical.push(("x-amz-content-sha256".to_string(), payload_hash.clone()));
    if let Some(token) = &creds.session_token {
        canonical.push(("x-amz-security-token".to_string(), token.clone()));
    }
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        // Skip auth-control headers we manage. `authorization` is the
        // output we're producing; the `x-amz-*` ones are added above.
        if matches!(
            lower.as_str(),
            "authorization"
                | "host"
                | "x-amz-date"
                | "x-amz-content-sha256"
                | "x-amz-security-token"
        ) {
            continue;
        }
        canonical.push((lower, value.trim().to_string()));
    }
    canonical.sort_by(|a, b| a.0.cmp(&b.0));

    let signed_headers_list = canonical
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_headers = canonical
        .iter()
        .map(|(n, v)| format!("{}:{}\n", n, v))
        .collect::<String>();

    let canonical_uri = canonical_uri(&parsed);
    let canonical_query = canonical_query(&parsed);

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        canonical_uri,
        canonical_query,
        canonical_headers,
        signed_headers_list,
        payload_hash,
    );

    let credential_scope = format!(
        "{}/{}/{}/aws4_request",
        date_stamp, creds.region, creds.service
    );

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        hex_sha256(canonical_request.as_bytes())
    );

    let signing_key = derive_signing_key(
        &creds.secret_access_key,
        &date_stamp,
        &creds.region,
        &creds.service,
    )?;

    let signature = hex::encode(
        hmac_sha256(&signing_key, string_to_sign.as_bytes())
            .map_err(|e| CloudAuthError::Signing(e.to_string()))?,
    );

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        creds.access_key_id, credential_scope, signed_headers_list, signature
    );

    let mut out = vec![
        SignedHeader {
            name: "Authorization".to_string(),
            value: authorization,
        },
        SignedHeader {
            name: "X-Amz-Date".to_string(),
            value: amz_date,
        },
        SignedHeader {
            name: "X-Amz-Content-Sha256".to_string(),
            value: payload_hash,
        },
    ];
    if let Some(token) = &creds.session_token {
        out.push(SignedHeader {
            name: "X-Amz-Security-Token".to_string(),
            value: token.clone(),
        });
    }
    Ok(out)
}

fn host_with_port(parsed: &Url, host: &str) -> String {
    match parsed.port() {
        // SigV4 omits default ports from the host header.
        Some(port)
            if !((parsed.scheme() == "https" && port == 443)
                || (parsed.scheme() == "http" && port == 80)) =>
        {
            format!("{}:{}", host, port)
        }
        _ => host.to_string(),
    }
}

fn canonical_uri(parsed: &Url) -> String {
    let path = parsed.path();
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn canonical_query(parsed: &Url) -> String {
    let Some(query) = parsed.query() else {
        return String::new();
    };
    // Split, decode, re-encode under SigV4's strict rules, sort.
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", encode_rfc3986(&k), encode_rfc3986(&v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// RFC 3986 unreserved-only percent-encoding used by SigV4 query canonicalization.
fn encode_rfc3986(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        let is_unreserved =
            b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~';
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn hex_sha256(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|e| format!("hmac key error: {}", e))?;
    mac.update(input);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn derive_signing_key(
    secret: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> CloudAuthResult<Vec<u8>> {
    let secret_prefixed = format!("AWS4{}", secret);
    let k_date = hmac_sha256(secret_prefixed.as_bytes(), date_stamp.as_bytes())
        .map_err(CloudAuthError::Signing)?;
    let k_region = hmac_sha256(&k_date, region.as_bytes()).map_err(CloudAuthError::Signing)?;
    let k_service = hmac_sha256(&k_region, service.as_bytes()).map_err(CloudAuthError::Signing)?;
    hmac_sha256(&k_service, b"aws4_request").map_err(CloudAuthError::Signing)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: a request with no body produces SHA256("") for the
    /// payload hash and signs without error.
    #[test]
    fn signs_empty_body_request() {
        let creds = AwsCredentials {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_string(),
            region: "us-east-1".to_string(),
            service: "ce".to_string(),
            session_token: None,
        };
        let headers = vec![(
            "Content-Type".to_string(),
            "application/x-amz-json-1.1".to_string(),
        )];
        let signed = sign_request(
            "POST",
            "https://ce.us-east-1.amazonaws.com/",
            &headers,
            b"",
            &creds,
        )
        .expect("sign");

        let names: Vec<&str> = signed.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"Authorization"));
        assert!(names.contains(&"X-Amz-Date"));
        assert!(names.contains(&"X-Amz-Content-Sha256"));

        // Empty body SHA256 is well-known.
        let body_hash = signed
            .iter()
            .find(|h| h.name == "X-Amz-Content-Sha256")
            .unwrap();
        assert_eq!(
            body_hash.value,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        // The signature line should reference our credential scope.
        let auth = signed.iter().find(|h| h.name == "Authorization").unwrap();
        assert!(auth.value.contains("AWS4-HMAC-SHA256"));
        assert!(auth.value.contains("Credential=AKIDEXAMPLE/"));
        assert!(auth.value.contains("/us-east-1/ce/aws4_request"));
        assert!(
            auth.value
                .contains("SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date")
        );
        assert!(auth.value.contains("Signature="));
    }

    /// Session tokens get reflected in both the headers and the signature.
    #[test]
    fn session_token_is_signed_and_returned() {
        let creds = AwsCredentials {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "secret".to_string(),
            region: "us-east-1".to_string(),
            service: "ce".to_string(),
            session_token: Some("FQoGZ...short".to_string()),
        };
        let signed = sign_request(
            "POST",
            "https://ce.us-east-1.amazonaws.com/",
            &[],
            b"{}",
            &creds,
        )
        .expect("sign");

        assert!(
            signed
                .iter()
                .any(|h| h.name == "X-Amz-Security-Token" && h.value == "FQoGZ...short")
        );
        let auth = signed.iter().find(|h| h.name == "Authorization").unwrap();
        assert!(auth.value.contains("x-amz-security-token"));
    }

    /// AWS reference test vector adapted from the SigV4 documentation —
    /// confirms the canonical-request / string-to-sign / signing-key
    /// pipeline produces the same intermediate signature bytes the AWS
    /// docs publish, locking down our HMAC chain against regressions.
    /// Test vectors: get-vanilla-query-order-key-case
    /// <https://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html>
    #[test]
    fn derives_known_signing_key() {
        // The AWS docs publish kSecret/kDate/kRegion/kService bytes for
        // the test vector secret. Confirm derive_signing_key matches.
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        )
        .expect("derive");
        // Published in the AWS SigV4 test vectors:
        // kSigning = HMAC(kService, "aws4_request")
        // = c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9
        let hex_key = hex::encode(&key);
        assert_eq!(
            hex_key,
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn rejects_malformed_credential_json() {
        let err = AwsCredentials::from_json("not json").unwrap_err();
        assert!(matches!(err, CloudAuthError::InvalidCredential(_)));
    }

    #[test]
    fn rejects_credential_missing_fields() {
        let err = AwsCredentials::from_json(r#"{"access_key_id": "k", "secret_access_key": "s"}"#)
            .unwrap_err();
        assert!(matches!(err, CloudAuthError::InvalidCredential(_)));
    }

    #[test]
    fn canonical_query_sorts_and_encodes() {
        let parsed = Url::parse("https://example.com/?b=2&a=1%20space&c=hello%2Fworld").unwrap();
        let q = canonical_query(&parsed);
        // Sorted ascending, space encoded as %20, slash encoded as %2F.
        assert_eq!(q, "a=1%20space&b=2&c=hello%2Fworld");
    }

    #[test]
    fn omits_default_port_from_host() {
        let parsed = Url::parse("https://example.com:443/").unwrap();
        assert_eq!(host_with_port(&parsed, "example.com"), "example.com");
        let parsed = Url::parse("https://example.com:8443/").unwrap();
        assert_eq!(host_with_port(&parsed, "example.com"), "example.com:8443");
    }
}
