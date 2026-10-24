//! Local certificate authority and per-site certificates.
//!
//! One CA is created per environment and trusted once by the user; every site then gets a
//! certificate for `name.tld` and `*.name.tld` signed by it.

use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use sha1::{Digest, Sha1};
use time::{Duration, OffsetDateTime};

use crate::{
    error::{Error, Result},
    layout::Layout,
};

const CA_NAME: &str = "lampRabit Local CA";
/// Browsers reject leaf certificates valid for more than ~398 days.
const SITE_VALID_DAYS: i64 = 397;
/// Re-issue site certificates this long before they expire.
const RENEW_BEFORE_DAYS: u64 = 30;

pub struct Ca {
    cert: Certificate,
    key: KeyPair,
}

pub fn ca_cert_path(layout: &Layout) -> PathBuf {
    layout.ssl().join("ca").join("lampRabit-ca.crt")
}

fn ca_paths(layout: &Layout) -> (PathBuf, PathBuf, PathBuf) {
    let dir = layout.ssl().join("ca");
    (ca_cert_path(layout), dir.join("lampRabit-ca.key"), dir.join("lampRabit-ca.sha1"))
}

fn ca_params() -> Result<CertificateParams> {
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(cert_err)?;
    params.distinguished_name.push(DnType::CommonName, CA_NAME);
    params.distinguished_name.push(DnType::OrganizationName, "lampRabit");
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(365 * 10);
    Ok(params)
}

fn cert_err(e: rcgen::Error) -> Error {
    Error::msg(format!("certificate error: {e}"))
}

/// SHA-1 thumbprint of the CA as stored, used to find it in OS trust stores.
pub fn ca_thumbprint(layout: &Layout) -> Option<String> {
    std::fs::read_to_string(ca_paths(layout).2).ok().map(|s| s.trim().to_string())
}

/// Loads the CA, creating it on first use. Returns whether it was newly created.
pub fn ensure_ca(layout: &Layout) -> Result<(Ca, bool)> {
    let (crt, key_path, sha) = ca_paths(layout);
    if crt.exists() && key_path.exists() {
        let key = KeyPair::from_pem(&std::fs::read_to_string(&key_path)?).map_err(cert_err)?;
        // Re-deriving the self-signed cert from the same key and name yields an issuer
        // that signs exactly like the stored one.
        let cert = ca_params()?.self_signed(&key).map_err(cert_err)?;
        return Ok((Ca { cert, key }, false));
    }

    std::fs::create_dir_all(crt.parent().unwrap())?;
    let key = KeyPair::generate().map_err(cert_err)?;
    let cert = ca_params()?.self_signed(&key).map_err(cert_err)?;
    let thumb: String = Sha1::digest(cert.der()).iter().map(|b| format!("{b:02X}")).collect();
    std::fs::write(&crt, cert.pem())?;
    std::fs::write(&key_path, key.serialize_pem())?;
    std::fs::write(&sha, thumb)?;
    // Site certificates from an older CA would no longer be trusted.
    let _ = std::fs::remove_dir_all(layout.ssl().join("sites"));
    Ok((Ca { cert, key }, true))
}

pub fn site_cert_paths(layout: &Layout, domain: &str) -> (PathBuf, PathBuf) {
    let dir = layout.ssl().join("sites");
    (dir.join(format!("{domain}.crt")), dir.join(format!("{domain}.key")))
}

fn needs_renewal(path: &Path) -> bool {
    let renew_after = std::time::Duration::from_secs((SITE_VALID_DAYS as u64 - RENEW_BEFORE_DAYS) * 86_400);
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > renew_after)
}

/// Issues (or renews) the certificate for `domain` and `*.domain`.
pub fn ensure_site_cert(layout: &Layout, ca: &Ca, domain: &str) -> Result<(PathBuf, PathBuf)> {
    let (crt, key_path) = site_cert_paths(layout, domain);
    if !needs_renewal(&crt) && key_path.exists() {
        return Ok((crt, key_path));
    }
    std::fs::create_dir_all(crt.parent().unwrap())?;
    let mut params =
        CertificateParams::new(vec![domain.to_string(), format!("*.{domain}")]).map_err(cert_err)?;
    params.distinguished_name.push(DnType::CommonName, domain);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(SITE_VALID_DAYS);
    let key = KeyPair::generate().map_err(cert_err)?;
    let cert = params.signed_by(&key, &ca.cert, &ca.key).map_err(cert_err)?;
    std::fs::write(&crt, cert.pem())?;
    std::fs::write(&key_path, key.serialize_pem())?;
    Ok((crt, key_path))
}

/// Whether the OS trust store contains this environment's CA.
pub async fn ca_trusted(layout: &Layout) -> bool {
    let Some(thumb) = ca_thumbprint(layout) else { return false };
    if cfg!(windows) {
        let mut cmd = tokio::process::Command::new("certutil");
        cmd.args(["-user", "-store", "Root", &thumb]);
        crate::procgroup::prepare(&mut cmd);
        cmd.output().await.is_ok_and(|o| o.status.success())
    } else if cfg!(target_os = "macos") {
        tokio::process::Command::new("security")
            .args(["find-certificate", "-a", "-Z", "-c", CA_NAME])
            .output()
            .await
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_uppercase().contains(&thumb))
    } else {
        false
    }
}

/// Adds the CA to the current user's trust store. Windows and macOS show their own
/// confirmation prompt; nothing needs administrator rights.
pub async fn trust_ca(layout: &Layout) -> Result<()> {
    let crt = ca_cert_path(layout);
    if !crt.exists() {
        ensure_ca(layout)?;
    }
    let output = if cfg!(windows) {
        let mut cmd = tokio::process::Command::new("certutil");
        cmd.args(["-user", "-addstore", "Root"]).arg(&crt);
        crate::procgroup::prepare(&mut cmd);
        cmd.output().await?
    } else if cfg!(target_os = "macos") {
        let keychain = dirs::home_dir().unwrap_or_default().join("Library/Keychains/login.keychain-db");
        tokio::process::Command::new("security")
            .args(["add-trusted-cert", "-r", "trustRoot", "-k"])
            .arg(keychain)
            .arg(&crt)
            .output()
            .await?
    } else {
        return Err(Error::msg(format!(
            "Add {} to your system and browser certificate stores (e.g. `sudo cp <file> /usr/local/share/ca-certificates/lampRabit.crt && sudo update-ca-certificates`).",
            crt.display()
        )));
    };
    if !output.status.success() {
        return Err(Error::msg("The certificate was not trusted (the prompt may have been cancelled)."));
    }
    Ok(())
}
