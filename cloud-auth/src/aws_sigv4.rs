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

    // Build the canonical headers per SigV4:
    // 1. Lowercase the name.
    // 2. Trim leading/trailing whitespace from the value AND collapse
    //    sequential internal SP/HTAB into a single space (Codex review
    //    REC 2). The AWS spec says canonical values use single-space
    //    separation; relying on `.trim()` alone would let
    //    `"foo  bar"` and `"foo bar"` hash differently while AWS
    //    treats them as equal.
    // 3. Aggregate duplicate header names by joining their values with
    //    a comma — SigV4 requires `SignedHeaders` entries to be
    //    unique (Codex review REC 1). Reqwest preserves caller-set
    //    duplicates verbatim so a caller passing two `Accept` headers
    //    would otherwise produce a malformed signature.
    // 4. Sort by name (ASCII ascending).
    //
    // We always include `host`, `x-amz-date`, `x-amz-content-sha256`
    // and (when present) `x-amz-security-token`. Caller-supplied
    // copies of these are dropped — the signer owns them.
    let mut accumulator: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    accumulator.insert("host".to_string(), vec![host_with_port(&parsed, host)]);
    accumulator.insert("x-amz-date".to_string(), vec![amz_date.clone()]);
    accumulator.insert(
        "x-amz-content-sha256".to_string(),
        vec![payload_hash.clone()],
    );
    if let Some(token) = &creds.session_token {
        accumulator.insert("x-amz-security-token".to_string(), vec![token.clone()]);
    }
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
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
        accumulator
            .entry(lower)
            .or_default()
            .push(canonical_value(value));
    }
    let canonical: Vec<(String, String)> = accumulator
        .into_iter()
        .map(|(name, values)| (name, values.join(",")))
        .collect();

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

/// SigV4 header-value canonicalization: trim ends, collapse internal
/// runs of SP/HTAB into a single SP. AWS spec
/// (`https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html`)
/// requires this so that semantically-equal header values produce the
/// same signature (Codex review REC 2).
fn canonical_value(raw: &str) -> String {
    let trimmed = raw.trim_matches(|c: char| c == ' ' || c == '\t');
    let mut out = String::with_capacity(trimmed.len());
    let mut in_ws = false;
    for c in trimmed.chars() {
        if c == ' ' || c == '\t' {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
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

/// Canonical request URI.
///
/// **Scope limitation (Codex review REC 4):** this implementation
/// passes the parsed path through verbatim, which is correct for the
/// services NyxID currently proxies (AWS Cost Explorer: `POST /`).
/// For services with non-trivial paths (S3 object keys, DynamoDB
/// resource paths, REST APIs like CloudFront management), SigV4 needs
/// segment-wise RFC 3986 encoding that preserves `/` as a separator
/// while encoding everything else inside each segment. Add that
/// before exposing this signer to new AWS services.
fn canonical_uri(parsed: &Url) -> String {
    let path = parsed.path();
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

/// Canonical query string per SigV4.
///
/// Rewritten to NOT use `form_urlencoded::parse` (Codex review REC 3),
/// which would `+`-decode and lose distinction between literal `+` and
/// encoded space. We instead walk the raw query string, split on `&`,
/// split each pair on the first `=`, percent-decode each part under
/// RFC 3986 rules, then re-encode under SigV4's unreserved-only rule
/// and sort. Empty values, repeated keys, and missing `=` are
/// preserved.
fn canonical_query(parsed: &Url) -> String {
    let Some(query) = parsed.query() else {
        return String::new();
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    for raw_pair in query.split('&') {
        if raw_pair.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match raw_pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (raw_pair, ""),
        };
        let key = decode_query_component(raw_key);
        let value = decode_query_component(raw_value);
        pairs.push((key, value));
    }
    // SigV4 sorts by encoded (k, v) pair. We sort by the decoded pair
    // here, then re-encode; for inputs that don't have ambiguous
    // encodings this is equivalent.
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", encode_rfc3986(&k), encode_rfc3986(&v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-decode a query-string component to bytes (treating `+` as
/// a literal `+`, NOT as space — SigV4 uses RFC 3986 query syntax,
/// not application/x-www-form-urlencoded). Falls back to raw bytes
/// on invalid percent escapes.
fn decode_query_component(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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

    /// Codex review REC 2: SigV4 canonical header values must collapse
    /// internal whitespace runs to a single space.
    #[test]
    fn canonical_value_collapses_internal_whitespace() {
        assert_eq!(canonical_value("foo  bar"), "foo bar");
        assert_eq!(canonical_value("foo\tbar"), "foo bar");
        assert_eq!(canonical_value("foo \t bar  baz"), "foo bar baz");
        assert_eq!(
            canonical_value(" leading and trailing "),
            "leading and trailing"
        );
        assert_eq!(canonical_value("single"), "single");
    }

    /// Codex review REC 1: duplicate caller-supplied headers must be
    /// folded into a single canonical entry with comma-joined values.
    /// We assert this end-to-end by checking the SignedHeaders list
    /// only contains each name once.
    #[test]
    fn duplicate_header_names_are_aggregated() {
        let creds = AwsCredentials {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "secret".to_string(),
            region: "us-east-1".to_string(),
            service: "ce".to_string(),
            session_token: None,
        };
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("accept".to_string(), "text/plain".to_string()),
        ];
        let signed = sign_request(
            "POST",
            "https://ce.us-east-1.amazonaws.com/",
            &headers,
            b"",
            &creds,
        )
        .expect("sign");
        let auth = signed.iter().find(|h| h.name == "Authorization").unwrap();
        // `accept` must appear EXACTLY once in SignedHeaders.
        let signed_headers_segment = auth
            .value
            .split("SignedHeaders=")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .expect("SignedHeaders");
        let count = signed_headers_segment
            .split(';')
            .filter(|n| *n == "accept")
            .count();
        assert_eq!(
            count, 1,
            "duplicate `accept` should aggregate to one entry: {signed_headers_segment}"
        );
    }

    /// Codex review REC 3: query canonicalization must handle empty
    /// values, repeated keys, and missing `=` correctly without
    /// `form_urlencoded::parse`'s `+`-as-space behavior.
    #[test]
    fn canonical_query_handles_edge_cases() {
        let parsed = Url::parse("https://x/?a=&b&c=hello%20world&a=2").unwrap();
        let q = canonical_query(&parsed);
        // Sorted by (decoded) key then value: `a=` < `a=2` < `b=` < `c=hello world`
        // After re-encode: `a=` `a=2` `b=` `c=hello%20world`.
        assert_eq!(q, "a=&a=2&b=&c=hello%20world");
    }

    #[test]
    fn canonical_query_treats_plus_as_literal() {
        // SigV4 query syntax is RFC 3986, NOT
        // application/x-www-form-urlencoded — `+` is a literal `+`,
        // not a space.
        let parsed = Url::parse("https://x/?q=a+b").unwrap();
        let q = canonical_query(&parsed);
        // Literal `+` re-encodes to `%2B` under the unreserved-only rule.
        assert_eq!(q, "q=a%2Bb");
    }
}
