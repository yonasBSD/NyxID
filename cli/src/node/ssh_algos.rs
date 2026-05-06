use std::borrow::Cow;
use std::str::FromStr;

use russh::keys::Algorithm;
use russh::{Preferred, cipher, kex, mac};

use super::config::SshAlgorithmPreferences;
use super::error::{Error, Result};

const HOST_KEY_SUPPORTED: &str = "ssh-ed25519, rsa-sha2-256, rsa-sha2-512, ssh-rsa, ecdsa-sha2-nistp256, ecdsa-sha2-nistp384, ecdsa-sha2-nistp521";

impl SshAlgorithmPreferences {
    pub fn is_empty(&self) -> bool {
        self.kex.is_none() && self.host_key.is_none() && self.cipher.is_none() && self.mac.is_none()
    }

    pub fn validate(&self) -> Result<()> {
        let _ = build_preferred(self)?;
        Ok(())
    }
}

pub fn build_preferred(prefs: &SshAlgorithmPreferences) -> Result<Preferred> {
    let mut preferred = Preferred::DEFAULT.clone();
    if let Some(list) = &prefs.kex {
        preferred.kex = Cow::Owned(resolve_kex_list(list)?);
    }
    if let Some(list) = &prefs.host_key {
        preferred.key = Cow::Owned(resolve_host_key_list(list)?);
    }
    if let Some(list) = &prefs.cipher {
        preferred.cipher = Cow::Owned(resolve_cipher_list(list)?);
    }
    if let Some(list) = &prefs.mac {
        preferred.mac = Cow::Owned(resolve_mac_list(list)?);
    }
    Ok(preferred)
}

pub fn resolve_kex_list(list: &[String]) -> Result<Vec<kex::Name>> {
    let supported = kex_supported_names();
    let mut resolved = resolve_name_list("kex", list, &supported)?;
    // Strict-kex and ext-info-c preserve safer negotiation even with a custom allowlist.
    push_if_missing(&mut resolved, kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT);
    push_if_missing(&mut resolved, kex::EXTENSION_SUPPORT_AS_CLIENT);
    Ok(resolved)
}

pub fn resolve_host_key_list(list: &[String]) -> Result<Vec<Algorithm>> {
    if list.is_empty() {
        return Err(empty_allowlist("host-key"));
    }

    list.iter()
        .map(|raw| {
            let input = raw.trim();
            if input.is_empty() {
                return Err(empty_algorithm_name("host-key"));
            }
            let algorithm = Algorithm::from_str(input).map_err(|_| {
                Error::Validation(format!(
                    "unknown ssh host-key algorithm '{input}' (supported: {})",
                    HOST_KEY_SUPPORTED
                ))
            })?;
            if is_allowed_host_key_algorithm(&algorithm) {
                Ok(algorithm)
            } else {
                Err(Error::Validation(format!(
                    "unknown ssh host-key algorithm '{}' (supported: {})",
                    algorithm.as_str(),
                    HOST_KEY_SUPPORTED
                )))
            }
        })
        .collect()
}

pub fn resolve_cipher_list(list: &[String]) -> Result<Vec<cipher::Name>> {
    resolve_name_list("cipher", list, cipher::ALL_CIPHERS)
}

pub fn resolve_mac_list(list: &[String]) -> Result<Vec<mac::Name>> {
    resolve_name_list("mac", list, mac::ALL_MAC_ALGORITHMS)
}

fn resolve_name_list<T>(category: &str, list: &[String], supported: &[&T]) -> Result<Vec<T>>
where
    T: Copy + AsRef<str>,
{
    if list.is_empty() {
        return Err(empty_allowlist(category));
    }

    let supported_names = supported
        .iter()
        .map(|name| name.as_ref())
        .collect::<Vec<_>>()
        .join(", ");

    list.iter()
        .map(|raw| {
            let input = raw.trim();
            if input.is_empty() {
                return Err(empty_algorithm_name(category));
            }
            if input.eq_ignore_ascii_case("none") {
                return Err(Error::Validation(format!(
                    "ssh {category} 'none' is not allowed"
                )));
            }
            supported
                .iter()
                .find(|name| name.as_ref() == input)
                .map(|name| **name)
                .ok_or_else(|| {
                    Error::Validation(format!(
                        "unknown ssh {category} algorithm '{input}' (supported: {supported_names})"
                    ))
                })
        })
        .collect()
}

fn kex_supported_names() -> Vec<&'static kex::Name> {
    let mut names = kex::ALL_KEX_ALGORITHMS.to_vec();
    names.push(&kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT);
    names.push(&kex::EXTENSION_SUPPORT_AS_CLIENT);
    names
}

fn push_if_missing<T>(target: &mut Vec<T>, value: T)
where
    T: Copy + PartialEq,
{
    if !target.contains(&value) {
        target.push(value);
    }
}

fn is_allowed_host_key_algorithm(algorithm: &Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::Ed25519 | Algorithm::Rsa { .. } | Algorithm::Ecdsa { .. }
    )
}

fn empty_allowlist(category: &str) -> Error {
    Error::Validation(format!("ssh {category} allowlist is empty"))
}

fn empty_algorithm_name(category: &str) -> Error {
    Error::Validation(format!(
        "ssh {category} allowlist contains an empty algorithm name"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn assert_validation_contains<T: std::fmt::Debug>(result: Result<T>, expected: &str) {
        match result {
            Err(Error::Validation(message)) => assert!(
                message.contains(expected),
                "expected validation error containing {expected:?}, got {message:?}"
            ),
            other => panic!("expected validation error containing {expected:?}, got {other:?}"),
        }
    }

    #[test]
    fn validate_accepts_valid_mixed_list_and_default_is_empty() {
        SshAlgorithmPreferences {
            kex: Some(strings(&["diffie-hellman-group-exchange-sha256"])),
            host_key: Some(strings(&["rsa-sha2-256", "ssh-rsa"])),
            cipher: Some(strings(&["aes256-ctr"])),
            mac: Some(strings(&["hmac-sha2-256"])),
        }
        .validate()
        .unwrap();
        assert!(SshAlgorithmPreferences::default().is_empty());
    }

    #[test]
    fn invalid_lists_are_rejected() {
        assert_validation_contains(resolve_kex_list(&[]), "ssh kex allowlist is empty");
        assert_validation_contains(
            resolve_host_key_list(&[]),
            "ssh host-key allowlist is empty",
        );
        assert_validation_contains(resolve_cipher_list(&[]), "ssh cipher allowlist is empty");
        assert_validation_contains(resolve_mac_list(&[]), "ssh mac allowlist is empty");
        assert_validation_contains(resolve_kex_list(&strings(&["bad-kex"])), "bad-kex");
        assert_validation_contains(
            resolve_host_key_list(&strings(&["bad-host-key"])),
            "bad-host-key",
        );
        assert_validation_contains(resolve_cipher_list(&strings(&["bad-cipher"])), "bad-cipher");
        assert_validation_contains(resolve_mac_list(&strings(&["bad-mac"])), "bad-mac");
        assert_validation_contains(resolve_host_key_list(&strings(&["ssh-dss"])), "ssh-dss");
        assert_validation_contains(
            resolve_host_key_list(&strings(&["unknown@example.com"])),
            "unknown@example.com",
        );
        assert_validation_contains(
            resolve_kex_list(&strings(&["none"])),
            "'none' is not allowed",
        );
        assert_validation_contains(
            resolve_cipher_list(&strings(&["NoNe"])),
            "'none' is not allowed",
        );
        assert_validation_contains(
            resolve_mac_list(&strings(&["NONE"])),
            "'none' is not allowed",
        );
    }

    #[test]
    fn host_key_list_accepts_supported_families() {
        let names = strings(&[
            "ssh-ed25519",
            "rsa-sha2-256",
            "rsa-sha2-512",
            "ssh-rsa",
            "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521",
        ]);

        let resolved = resolve_host_key_list(&names).unwrap();
        assert_eq!(resolved.len(), names.len());
    }

    #[test]
    fn kex_list_appends_required_client_markers() {
        let resolved =
            resolve_kex_list(&strings(&["diffie-hellman-group-exchange-sha256"])).unwrap();

        assert!(resolved.contains(&kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT));
        assert!(resolved.contains(&kex::EXTENSION_SUPPORT_AS_CLIENT));
    }
}
