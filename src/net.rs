//! Shared HTTP client setup.
//!
//! Some panel hosts (minimal containers, or hosts that inspect HTTPS traffic) make
//! the built-in Mozilla root store fail with `invalid peer certificate: UnknownIssuer`.
//! Every `reqwest` client in the bot is built through [`builder`], which can add
//! extra trusted CAs and tweak proxy / TLS behaviour from the environment:
//!
//! * `OMNICORE_CA_FILE=/path/ca.pem`  extra CA bundle (also honours `SSL_CERT_FILE`);
//!   the usual system bundle paths are picked up automatically when present
//! * `OMNICORE_NO_PROXY=1`            ignore HTTP(S)_PROXY variables
//! * `OMNICORE_INSECURE_TLS=1`        LAST RESORT: skip certificate verification

use std::sync::OnceLock;

fn flag(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

fn extra_roots() -> &'static Vec<reqwest::Certificate> {
    static ROOTS: OnceLock<Vec<reqwest::Certificate>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut paths: Vec<String> = Vec::new();
        for key in ["OMNICORE_CA_FILE", "SSL_CERT_FILE"] {
            if let Ok(v) = std::env::var(key) {
                if !v.trim().is_empty() {
                    paths.push(v.trim().to_string());
                }
            }
        }
        for p in [
            "/etc/ssl/certs/ca-certificates.crt",
            "/etc/pki/tls/certs/ca-bundle.crt",
            "/etc/ssl/cert.pem",
        ] {
            paths.push(p.to_string());
        }

        let mut certs = Vec::new();
        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let before = certs.len();
            const END: &str = "-----END CERTIFICATE-----";
            for chunk in text.split_inclusive(END) {
                if !chunk.contains("-----BEGIN CERTIFICATE-----") {
                    continue;
                }
                if let Ok(cert) = reqwest::Certificate::from_pem(chunk.trim().as_bytes()) {
                    certs.push(cert);
                }
            }
            if certs.len() > before {
                tracing::info!(path = %path, added = certs.len() - before, "extra CA certificates loaded");
            }
        }
        if flag("OMNICORE_INSECURE_TLS") {
            tracing::warn!("OMNICORE_INSECURE_TLS is on — TLS certificates are NOT verified");
        }
        certs
    })
}

/// Drop-in replacement for `reqwest::Client::builder()`.
pub fn builder() -> reqwest::ClientBuilder {
    let mut b = reqwest::Client::builder();
    for cert in extra_roots() {
        b = b.add_root_certificate(cert.clone());
    }
    if flag("OMNICORE_NO_PROXY") {
        b = b.no_proxy();
    }
    if flag("OMNICORE_INSECURE_TLS") {
        b = b.danger_accept_invalid_certs(true);
    }
    b
}

/// Drop-in replacement for `reqwest::Client::new()`.
pub fn client() -> reqwest::Client {
    builder().build().unwrap_or_else(|_| reqwest::Client::new())
}
