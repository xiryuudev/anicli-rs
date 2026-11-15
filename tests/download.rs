//! Uji unduhan langsung + progress menggunakan server HTTP lokal.

use std::path::PathBuf;

use anicli_rs::download::{format_bytes, is_direct_file, safe_file_name, DownloadSpec};
use anicli_rs::ui::AppEvent;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::unbounded_channel;
use url::Url;

/// Server lokal: `/file.bin` mengirim `payload` dengan Content-Length.
async fn serve(payload: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                    payload.len()
                );
                let _ = socket.write_all(header.as_bytes()).await;
                let _ = socket.write_all(payload).await;
                let _ = socket.flush().await;
            });
        }
    });
    format!("http://{addr}/file.mkv")
}

#[tokio::test]
async fn direct_download_writes_file_and_reports_progress() {
    static PAYLOAD: &[u8] = b"anicli-rs download probe payload 0123456789";
    let url = Url::parse(&serve(PAYLOAD).await).unwrap();
    assert!(is_direct_file(&url));

    let dir = tempfile::TempDir::new().unwrap();
    let dest = dir.path().join("unduhan/file.mkv");
    let (tx, mut rx) = unbounded_channel();

    let spec = DownloadSpec {
        url,
        dest: dest.clone(),
        referer: None,
        user_agent: None,
    };
    anicli_rs::download::run(7, spec, tx).await;

    let mut started = false;
    let mut finished = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            AppEvent::DownloadStarted { job, total, .. } => {
                assert_eq!(job, 7);
                assert_eq!(total, Some(PAYLOAD.len() as u64));
                started = true;
            }
            AppEvent::DownloadProgress { downloaded, .. } => {
                assert!(downloaded > 0);
            }
            AppEvent::DownloadDone { path, .. } => {
                assert_eq!(path, dest);
                finished = true;
            }
            AppEvent::DownloadFailed { message, .. } => panic!("gagal: {message}"),
            _ => {}
        }
    }
    assert!(started, "DownloadStarted harus terkirim");
    assert!(finished, "DownloadDone harus terkirim");
    let written = std::fs::read(&dest).unwrap();
    assert_eq!(written, PAYLOAD);
}

#[tokio::test]
async fn failure_is_reported() {
    // Port tertutup → permintaan gagal.
    let url = Url::parse("http://127.0.0.1:1/file.bin").unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let dest: PathBuf = dir.path().join("x.bin");
    let (tx, mut rx) = unbounded_channel();
    let spec = DownloadSpec {
        url,
        dest,
        referer: None,
        user_agent: None,
    };
    anicli_rs::download::run(9, spec, tx).await;
    let mut failed = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::DownloadFailed { job, message } = event {
            assert_eq!(job, 9);
            assert!(!message.is_empty());
            failed = true;
        }
    }
    assert!(failed, "DownloadFailed harus terkirim");
}

#[test]
fn helpers_behave() {
    assert_eq!(safe_file_name("a/b", "mkv"), "a-b.mkv");
    assert_eq!(format_bytes(1024), "1 KB");
}
