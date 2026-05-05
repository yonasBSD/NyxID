# Releasing NyxID

This document covers the release path for the `nyxid` CLI binaries produced by CI.

## Release Trigger

Releases are tag-driven. From an up-to-date `main` checkout:

```bash
git pull --ff-only origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

Use the same `X.Y.Z` version as `cli/Cargo.toml`. Do not reuse a published tag for different bits; cut a new patch tag if the release contents need to change.

## What CI Builds

The `Release` workflow runs cargo-dist and publishes the `nyxid-cli` package for:

- `x86_64-unknown-linux-gnu` on `ubuntu-24.04`
- `aarch64-unknown-linux-gnu` on `ubuntu-24.04-arm`
- `x86_64-apple-darwin` on `macos-latest`
- `aarch64-apple-darwin` on `macos-latest`
- `x86_64-pc-windows-msvc` on `windows-latest`

Linux release runners install `libdbus-1-dev` and `pkg-config` because the CLI uses the keyring crate's Secret Service backend on Linux. The wizard bundle is already committed under `cli/src/wizard/assets/`, so release builds do not need Node.

cargo-dist publishes:

- Platform archives: `nyxid-cli-<target>.tar.gz` for Unix and `nyxid-cli-<target>.zip` for Windows
- SHA-256 checksums
- `nyxid-cli-installer.sh`
- `nyxid-cli-installer.ps1`
- Generated GitHub release notes, with NyxID changelog and container image notes appended by the workflow

## Attestations

The release workflow uses GitHub Artifact Attestations with Sigstore-backed keyless signing. There are no minisign keys, no signing keys in the repository, and no signing material in GitHub Secrets.

After cargo-dist uploads the release artifacts, the workflow runs:

```yaml
actions/attest-build-provenance@v1
```

The workflow permissions include:

- `contents: write`
- `id-token: write`
- `attestations: write`

The signer identity is the workflow run itself. For tag `vX.Y.Z`, the CLI updater expects this workflow identity:

```text
https://github.com/ChronoAIProject/NyxID/.github/workflows/release.yml@refs/tags/vX.Y.Z
```

## Self-Update Verification

`nyxid update` does not compile by default. It:

1. Resolves `--version X.Y.Z` to `vX.Y.Z`, or fetches the latest GitHub release.
2. Selects the asset matching the compiled host target.
3. Downloads the archive.
4. Fetches the GitHub Artifact Attestation for the archive's SHA-256 digest.
5. Verifies the Sigstore bundle and exact workflow identity.
6. Extracts `nyxid` or `nyxid.exe`.
7. Replaces the running binary with `self-replace`.
8. Re-execs `nyxid update --skills-only` so the new binary owns skill updates.

If no prebuilt asset exists for the host target, the updater clearly falls back to:

```bash
cargo install --git https://github.com/ChronoAIProject/NyxID nyxid-cli --force --locked
```

Verification failures never fall back to source automatically. They fail closed unless the user explicitly passes `--insecure-skip-verify`.

## Installer URLs

End users can install the latest prebuilt CLI with:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/ChronoAIProject/NyxID/releases/latest/download/nyxid-cli-installer.sh | sh
```

Windows users can use the PowerShell installer from the same release:

```powershell
irm https://github.com/ChronoAIProject/NyxID/releases/latest/download/nyxid-cli-installer.ps1 | iex
```

The repository skill installer at `skills/nyxid/scripts/install.sh` wraps the shell installer and falls back to `cargo install` only when a usable prebuilt binary is not available for the host.

## If Release CI Fails

First determine where it failed:

- Build job failed before publishing: fix `main`, then either rerun the workflow if the failure was transient or cut a new patch tag after the fix.
- Host/upload job failed before a GitHub release exists: rerun the workflow for transient infrastructure failures; otherwise fix `main` and cut a new patch tag.
- A GitHub release or artifacts were partially published: remove the broken release from GitHub, leave the failed workflow logs intact for auditability, fix `main`, and cut a new patch tag. Do not replace signed artifacts in-place under the same published tag.
- Attestation failed: inspect the `Attest release artifacts` step and workflow permissions. Do not publish binaries without attestations.

Before retrying a release, validate locally:

```bash
dist plan --output-format=json --tag=vX.Y.Z
cargo build -p nyxid-cli
cargo test -p nyxid-cli
cargo clippy -p nyxid-cli --all-targets -- -D warnings
cargo fmt --all -- --check
```
