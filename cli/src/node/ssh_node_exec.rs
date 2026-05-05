use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, decode_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc;

use super::credentials::ssh_keys::SshKeyEntry;

pub const SSH_NODE_KEY_MISSING_CODE: u32 = 1011;
pub const SSH_HOST_KEY_MISMATCH_CODE: u32 = 1012;
pub const SSH_NODE_EXEC_CHANNEL_CLOSED_CODE: u32 = 1013;

const SSH_NODE_EXEC_MAX_OUTPUT_BYTES: usize = 1_048_576;

#[derive(Debug)]
pub struct SshNodeExecOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

#[derive(Debug)]
pub struct SshNodeExecError {
    pub code: u32,
    pub message: String,
}

#[derive(Debug)]
pub enum SshNodeShellControl {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

#[derive(Debug)]
pub enum SshNodeShellEvent {
    Started,
    Data(Vec<u8>),
    Closed(Option<SshNodeExecError>),
}

impl SshNodeExecError {
    pub fn missing_key(service_slug: &str, principal: &str) -> Self {
        Self {
            code: SSH_NODE_KEY_MISSING_CODE,
            message: format!(
                "SSH node key missing for service '{service_slug}' principal '{principal}'"
            ),
        }
    }

    fn host_key_mismatch(expected: &str, actual: &str) -> Self {
        Self {
            code: SSH_HOST_KEY_MISMATCH_CODE,
            message: format!("SSH host key mismatch: expected {expected}, got {actual}"),
        }
    }

    fn channel_closed(message: impl Into<String>) -> Self {
        Self {
            code: SSH_NODE_EXEC_CHANNEL_CLOSED_CODE,
            message: message.into(),
        }
    }
}

#[derive(Clone)]
struct HostKeyVerifier {
    expected_sha256: Option<String>,
    observed_sha256: Arc<Mutex<Option<String>>>,
}

impl client::Handler for HostKeyVerifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = host_key_sha256(server_public_key);
        *self.observed_sha256.lock().expect("host key lock poisoned") = Some(fingerprint.clone());

        let Some(expected) = self.expected_sha256.as_deref() else {
            tracing::info!(host_key_sha256 = %fingerprint, "Accepted SSH host key without pin");
            return Ok(true);
        };

        Ok(normalize_sha256_fingerprint(expected) == normalize_sha256_fingerprint(&fingerprint))
    }
}

pub async fn exec_command(
    entry: SshKeyEntry,
    command: String,
    timeout_secs: u64,
) -> Result<SshNodeExecOutput, SshNodeExecError> {
    let timeout = Duration::from_secs(timeout_secs.clamp(1, 300));
    let started_at = Instant::now();
    match tokio::time::timeout(timeout, exec_command_inner(entry, command)).await {
        Ok(result) => result,
        Err(_) => Ok(SshNodeExecOutput {
            exit_code: -1,
            stdout: Vec::new(),
            stderr: b"Command execution timed out".to_vec(),
            duration_ms: started_at.elapsed().as_millis() as u64,
            timed_out: true,
        }),
    }
}

pub async fn scan_host_key_sha256(
    host: &str,
    port: u16,
    timeout_secs: u64,
) -> Result<String, SshNodeExecError> {
    let observed_sha256 = Arc::new(Mutex::new(None));
    let handler = HostKeyVerifier {
        expected_sha256: None,
        observed_sha256: observed_sha256.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs.clamp(1, 300))),
        ..Default::default()
    });
    let addr = (host, port);
    let session = tokio::time::timeout(
        Duration::from_secs(timeout_secs.clamp(1, 300)),
        client::connect(config, addr, handler),
    )
    .await
    .map_err(|_| SshNodeExecError::channel_closed("ssh host-key scan timed out"))?
    .map_err(|error| {
        SshNodeExecError::channel_closed(format!("ssh host-key scan failed: {error}"))
    })?;

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    observed_sha256
        .lock()
        .expect("host key lock poisoned")
        .clone()
        .ok_or_else(|| SshNodeExecError::channel_closed("ssh host-key scan returned no host key"))
}

pub async fn test_connection(
    entry: SshKeyEntry,
    timeout_secs: u64,
) -> Result<(), SshNodeExecError> {
    let timeout = Duration::from_secs(timeout_secs.clamp(1, 300));
    match tokio::time::timeout(timeout, async move {
        let session = connect_authenticated(&entry).await?;
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        Ok(())
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err(SshNodeExecError::channel_closed(
            "ssh connection test timed out",
        )),
    }
}

async fn exec_command_inner(
    entry: SshKeyEntry,
    command: String,
) -> Result<SshNodeExecOutput, SshNodeExecError> {
    let started_at = Instant::now();
    let session = connect_authenticated(&entry).await?;

    let mut channel = session.channel_open_session().await.map_err(|error| {
        SshNodeExecError::channel_closed(format!("open session failed: {error}"))
    })?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| {
            SshNodeExecError::channel_closed(format!("exec request failed: {error}"))
        })?;

    let mut exit_code: Option<i32> = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    loop {
        let Some(message) = channel.wait().await else {
            break;
        };

        match message {
            ChannelMsg::Data { data } => append_capped(&mut stdout, data.as_ref()),
            ChannelMsg::ExtendedData { data, ext: 1 } => {
                append_capped(&mut stderr, data.as_ref());
            }
            ChannelMsg::ExtendedData { data, .. } => {
                append_capped(&mut stderr, data.as_ref());
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(i32::try_from(exit_status).unwrap_or(-1));
            }
            ChannelMsg::ExitSignal { error_message, .. } => {
                append_capped(&mut stderr, error_message.as_bytes());
                exit_code = Some(-1);
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;

    Ok(SshNodeExecOutput {
        exit_code: exit_code.unwrap_or(-1),
        stdout,
        stderr,
        duration_ms: started_at.elapsed().as_millis() as u64,
        timed_out: false,
    })
}

pub async fn run_shell(
    entry: SshKeyEntry,
    term: String,
    cols: u32,
    rows: u32,
    mut control_rx: mpsc::Receiver<SshNodeShellControl>,
    event_tx: mpsc::Sender<SshNodeShellEvent>,
) {
    if let Err(error) = run_shell_inner(entry, term, cols, rows, &mut control_rx, &event_tx).await {
        let _ = event_tx.send(SshNodeShellEvent::Closed(Some(error))).await;
    }
}

async fn run_shell_inner(
    entry: SshKeyEntry,
    term: String,
    cols: u32,
    rows: u32,
    control_rx: &mut mpsc::Receiver<SshNodeShellControl>,
    event_tx: &mpsc::Sender<SshNodeShellEvent>,
) -> Result<(), SshNodeExecError> {
    let session = connect_authenticated(&entry).await?;
    let channel = session.channel_open_session().await.map_err(|error| {
        SshNodeExecError::channel_closed(format!("open shell session failed: {error}"))
    })?;

    channel
        .request_pty(
            true,
            term.as_str(),
            cols.clamp(10, 500),
            rows.clamp(2, 200),
            0,
            0,
            &[],
        )
        .await
        .map_err(|error| {
            SshNodeExecError::channel_closed(format!("pty request failed: {error}"))
        })?;
    channel.request_shell(true).await.map_err(|error| {
        SshNodeExecError::channel_closed(format!("shell request failed: {error}"))
    })?;

    let (mut read_half, write_half) = channel.split();
    event_tx
        .send(SshNodeShellEvent::Started)
        .await
        .map_err(|_| SshNodeExecError::channel_closed("shell event receiver closed"))?;

    loop {
        tokio::select! {
            control = control_rx.recv() => {
                match control {
                    Some(SshNodeShellControl::Data(bytes)) => {
                        write_half.data(&bytes[..]).await.map_err(|error| {
                            SshNodeExecError::channel_closed(format!("shell write failed: {error}"))
                        })?;
                    }
                    Some(SshNodeShellControl::Resize { cols, rows }) => {
                        write_half
                            .window_change(cols.clamp(10, 500), rows.clamp(2, 200), 0, 0)
                            .await
                            .map_err(|error| {
                                SshNodeExecError::channel_closed(format!("shell resize failed: {error}"))
                            })?;
                    }
                    Some(SshNodeShellControl::Close) | None => {
                        let _ = write_half.eof().await;
                        let _ = write_half.close().await;
                        break;
                    }
                }
            }
            message = read_half.wait() => {
                let Some(message) = message else {
                    break;
                };
                match message {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        event_tx
                            .send(SshNodeShellEvent::Data(data.to_vec()))
                            .await
                            .map_err(|_| SshNodeExecError::channel_closed("shell event receiver closed"))?;
                    }
                    ChannelMsg::ExitStatus { .. } | ChannelMsg::ExitSignal { .. } | ChannelMsg::Close => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    let _ = event_tx.send(SshNodeShellEvent::Closed(None)).await;
    Ok(())
}

async fn connect_authenticated(
    entry: &SshKeyEntry,
) -> Result<client::Handle<HostKeyVerifier>, SshNodeExecError> {
    let passphrase = entry.passphrase.as_ref().map(|value| value.as_str());
    let key_pair =
        decode_secret_key(entry.private_key_pem.as_str(), passphrase).map_err(|error| {
            SshNodeExecError::channel_closed(format!("invalid private key: {error}"))
        })?;

    let observed_sha256 = Arc::new(Mutex::new(None));
    let handler = HostKeyVerifier {
        expected_sha256: entry.host_key_sha256.clone(),
        observed_sha256: observed_sha256.clone(),
    };

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let addr = (entry.target_host.as_str(), entry.target_port);
    let mut session = match client::connect(config, addr, handler).await {
        Ok(session) => session,
        Err(error) => {
            if let (Some(expected), Some(actual)) = (
                entry.host_key_sha256.as_deref(),
                observed_sha256
                    .lock()
                    .expect("host key lock poisoned")
                    .as_deref(),
            ) && normalize_sha256_fingerprint(expected) != normalize_sha256_fingerprint(actual)
            {
                return Err(SshNodeExecError::host_key_mismatch(
                    &format_sha256_fingerprint(expected),
                    actual,
                ));
            }

            return Err(SshNodeExecError::channel_closed(format!(
                "ssh connect failed: {error}"
            )));
        }
    };

    let auth_result = session
        .authenticate_publickey(
            entry.principal.clone(),
            PrivateKeyWithHashAlg::new(
                Arc::new(key_pair),
                session
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|error| {
                        SshNodeExecError::channel_closed(format!(
                            "failed to negotiate RSA signature hash: {error}"
                        ))
                    })?
                    .flatten(),
            ),
        )
        .await
        .map_err(|error| SshNodeExecError::channel_closed(format!("ssh auth failed: {error}")))?;

    if !auth_result.success() {
        return Err(SshNodeExecError::channel_closed(
            "ssh auth failed: public key rejected",
        ));
    }

    Ok(session)
}

pub fn host_key_sha256(public_key: &russh::keys::ssh_key::PublicKey) -> String {
    public_key
        .fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
        .to_string()
}

pub fn normalize_sha256_fingerprint(fingerprint: &str) -> String {
    let trimmed = fingerprint.trim();
    let without_prefix = trimmed
        .strip_prefix("SHA256:")
        .or_else(|| trimmed.strip_prefix("sha256:"))
        .unwrap_or(trimmed);
    without_prefix.trim_end_matches('=').to_string()
}

pub fn format_sha256_fingerprint(fingerprint: &str) -> String {
    format!("SHA256:{}", normalize_sha256_fingerprint(fingerprint))
}

fn append_capped(target: &mut Vec<u8>, chunk: &[u8]) {
    let remaining = SSH_NODE_EXEC_MAX_OUTPUT_BYTES.saturating_sub(target.len());
    if remaining > 0 {
        target.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sha256_fingerprint_prefix_and_padding() {
        assert_eq!(
            normalize_sha256_fingerprint("SHA256:abc123=="),
            "abc123".to_string()
        );
        assert_eq!(
            normalize_sha256_fingerprint("sha256:abc123"),
            "abc123".to_string()
        );
        assert_eq!(format_sha256_fingerprint("abc123=="), "SHA256:abc123");
    }

    #[test]
    fn append_capped_respects_output_limit() {
        let mut output = vec![b'a'; SSH_NODE_EXEC_MAX_OUTPUT_BYTES - 2];
        append_capped(&mut output, b"bcdef");
        assert_eq!(output.len(), SSH_NODE_EXEC_MAX_OUTPUT_BYTES);
        assert!(output.ends_with(b"bc"));
    }
}
