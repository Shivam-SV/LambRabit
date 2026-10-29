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
        // The system store holds a copy of our certificate once trusted.
        let ours = std::fs::read(ca_cert_path(layout)).ok();
        linux_store().is_some_and(|(dir, _)| ours.is_some() && std::fs::read(dir.join(LINUX_FILE)).ok() == ours)
    }
}

const LINUX_FILE: &str = "lampRabit.crt";

/// The distribution's folder for extra trusted CAs and the command that rebuilds the
/// store from it: Debian/Ubuntu, Fedora/RHEL, Arch, openSUSE.
fn linux_store() -> Option<(PathBuf, &'static str)> {
    [
        ("/usr/local/share/ca-certificates", "update-ca-certificates"),
        ("/etc/pki/ca-trust/source/anchors", "update-ca-trust"),
        ("/etc/ca-certificates/trust-source/anchors", "update-ca-trust"),
        ("/etc/pki/trust/anchors", "update-ca-certificates"),
    ]
    .into_iter()
    .map(|(dir, cmd)| (PathBuf::from(dir), cmd))
    .find(|(dir, _)| dir.is_dir())
}

/// Chrome, Chromium and Firefox on Linux ignore the system store and read NSS databases:
/// the shared ~/.pki/nssdb and one per Firefox profile. Needs `certutil` (libnss3-tools).
/// Returns a note when browsers couldn't be covered.
async fn trust_in_browsers(crt: &Path) -> Option<String> {
    let home = dirs::home_dir()?;
    if tokio::process::Command::new("certutil").arg("-H").output().await.is_err() {
        return Some(
            "Trusted by the system. Chrome and Firefox keep their own certificate lists: install certutil (sudo apt install libnss3-tools, or nss-tools on Fedora) and click Trust again to add it there.".into(),
        );
    }
    let certutil = |db: &Path| {
        let mut cmd = tokio::process::Command::new("certutil");
        cmd.arg("-d").arg(format!("sql:{}", db.display()));
        cmd
    };
    let shared = home.join(".pki/nssdb");
    if !shared.join("cert9.db").exists() {
        let _ = std::fs::create_dir_all(&shared);
        let _ = certutil(&shared).args(["-N", "--empty-password"]).output().await;
    }
    let mut dbs = vec![shared];
    for profiles in [".mozilla/firefox", "snap/firefox/common/.mozilla/firefox", ".var/app/org.mozilla.firefox/.mozilla/firefox"] {
        for entry in std::fs::read_dir(home.join(profiles)).into_iter().flatten().flatten() {
            if entry.path().join("cert9.db").exists() {
                dbs.push(entry.path());
            }
        }
    }
    for db in dbs {
        let _ = certutil(&db).args(["-A", "-t", "C,,", "-n", CA_NAME, "-i"]).arg(crt).output().await;
    }
    None
}

/// Adds the CA to the current user's trust store. Windows and macOS show their own
/// confirmation prompt; Linux asks for administrator rights for the system store. Returns
/// a note when part of it couldn't be done.
pub async fn trust_ca(layout: &Layout) -> Result<Option<String>> {
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
        let (dir, update) = linux_store().ok_or_else(|| {
            Error::msg(format!("Couldn't find this system's certificate store. Add {} to it by hand.", crt.display()))
        })?;
        let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
        let script = format!("install -m 644 {} {} && {update}", quote(&crt), quote(&dir.join(LINUX_FILE)));
        tokio::process::Command::new("pkexec").args(["sh", "-c", &script]).output().await?
    };
    if !output.status.success() {
        return Err(Error::msg("The certificate was not trusted (the prompt may have been cancelled)."));
    }
    Ok(if cfg!(target_os = "linux") { trust_in_browsers(&crt).await } else { None })
}
