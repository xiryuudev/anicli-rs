//! Penjaga regresi: semua client HTTP wajib lewat `http::client_builder()`.
//!
//! `reqwest` dibangun dengan `rustls-no-provider`. Client yang dibuat langsung
//! (`reqwest::Client::builder()`) akan jatuh ke `rustls-platform-verifier`, yang di
//! Android panic karena tidak punya JNI/Context — persis bug `anicli update` 0.7.0.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_http_module_constructs_reqwest_clients_directly() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);

    let mut offenders = Vec::new();
    for file in &files {
        if file.ends_with("src/http.rs") {
            continue; // tempat `client_builder()` didefinisikan
        }
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        if text.contains("reqwest::Client::builder()") {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "client HTTP harus memakai crate::http::client_builder() (TLS webpki):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn client_builder_produces_working_client() {
    anicli_rs::http::ensure_tls_provider();
    let client = anicli_rs::http::client_builder().build();
    assert!(
        client.is_ok(),
        "client_builder harus bisa build: {client:?}"
    );
}
