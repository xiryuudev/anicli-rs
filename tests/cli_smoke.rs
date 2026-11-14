//! Smoke test end-to-end untuk binary `anicli` (Tahap 3).
//!
//! Menguji kontrak yang paling mudah rusak: pembuatan & pembacaan ulang config,
//! presedensi override CLI, exit code untuk error yang bisa diperbaiki user,
//! dan bukti bahwa logging benar-benar menulis file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("tempdir"),
        }
    }

    fn config_file(&self) -> PathBuf {
        self.dir.path().join("config").join("config.toml")
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_anicli"))
            .args(args)
            .env("ANICLI_CONFIG", self.config_file())
            .env("ANICLI_DATA_DIR", self.data_dir())
            // Jangan biarkan environment developer mengubah hasil test.
            .env_remove("RUST_LOG")
            .env_remove("ANICLI_SOURCE")
            .env_remove("ANICLI_RESOLUTION")
            .env_remove("ANICLI_PLAYER")
            .env_remove("ANICLI_LOG")
            .output()
            .expect("menjalankan binary anicli")
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("anicli-rs") && name.ends_with(".log"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

#[test]
fn version_and_help_work() {
    let sandbox = Sandbox::new();

    let version = sandbox.run(&["--version"]);
    assert!(version.status.success(), "{}", stderr(&version));
    let text = stdout(&version);
    assert!(text.contains("anicli"), "{text}");
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");

    let help = sandbox.run(&["--help"]);
    assert!(help.status.success(), "{}", stderr(&help));
    let text = stdout(&help);
    assert!(text.contains("config"), "{text}");
    assert!(text.contains("--no-tui"), "{text}");
    assert!(text.contains("--resolution"), "{text}");
}

#[test]
fn config_path_prints_locations_without_creating_anything() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&["config", "path"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains(&sandbox.config_file().display().to_string()),
        "{text}"
    );
    assert!(
        text.contains(&sandbox.data_dir().display().to_string()),
        "{text}"
    );
    assert!(text.contains("logs"), "{text}");
    assert!(
        !sandbox.config_file().exists(),
        "config path tidak boleh menulis file"
    );
}

#[test]
fn config_init_creates_template_then_refuses_and_force_overwrites() {
    let sandbox = Sandbox::new();

    let first = sandbox.run(&["config", "init"]);
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(
        stdout(&first).contains("config dibuat"),
        "{}",
        stdout(&first)
    );
    let created = std::fs::read_to_string(sandbox.config_file()).expect("config harus ada");
    assert!(created.contains("config_version = 1"), "{created}");
    assert!(created.contains("[sources.samehadaku]"), "{created}");

    let second = sandbox.run(&["config", "init"]);
    assert!(second.status.success());
    assert!(stdout(&second).contains("sudah ada"), "{}", stdout(&second));

    std::fs::write(sandbox.config_file(), "config_version = 1\n").unwrap();
    let forced = sandbox.run(&["config", "init", "--force"]);
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert!(stdout(&forced).contains("ditimpa"), "{}", stdout(&forced));
    let rewritten = std::fs::read_to_string(sandbox.config_file()).unwrap();
    assert!(rewritten.contains("[playback]"), "{rewritten}");
}

#[test]
fn first_run_creates_config_data_dirs_and_log_file() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&[]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let text = stdout(&output);
    assert!(text.contains("samehadaku"), "{text}");
    assert!(text.contains("player"), "{text}");
    assert!(text.contains("ring buffer"), "{text}");

    assert!(
        sandbox.config_file().exists(),
        "config harus dibuat otomatis"
    );
    assert!(
        sandbox.data_dir().join("logs").is_dir(),
        "direktori log harus dibuat"
    );

    let logs = log_files(&sandbox.data_dir().join("logs"));
    assert_eq!(
        logs.len(),
        1,
        "harus ada tepat satu file log harian: {logs:?}"
    );
    let content = std::fs::read_to_string(&logs[0]).expect("log bisa dibaca");
    assert!(!content.is_empty(), "file log tidak boleh kosong");
    assert!(
        content.contains("anicli-rs mulai"),
        "log harus memuat startup: {content}"
    );
}

#[test]
fn cli_overrides_win_without_mutating_config_file() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&[
        "--source",
        "otakudesu",
        "--resolution",
        "1080p",
        "--player",
        "vlc",
        "--no-tui",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("otakudesu"), "{text}");
    assert!(text.contains("1080p"), "{text}");
    assert!(text.contains("vlc"), "{text}");

    let saved = std::fs::read_to_string(sandbox.config_file()).unwrap();
    assert!(saved.contains("default_source = \"samehadaku\""), "{saved}");
    assert!(saved.contains("default_resolution = \"720p\""), "{saved}");
}

#[test]
fn invalid_resolution_exits_with_config_code() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&["--resolution", "hd"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(
        err.contains("720p"),
        "pesan harus memberi contoh valid: {err}"
    );
}

#[test]
fn invalid_config_exits_with_config_code_and_names_the_key() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.config_file().parent().unwrap()).unwrap();
    std::fs::write(
        sandbox.config_file(),
        "config_version = 1\n[general]\ndefault_source = \"nope\"\n",
    )
    .unwrap();

    let output = sandbox.run(&["--no-tui"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("general.default_source"), "{err}");
}

#[test]
fn unknown_source_override_is_rejected() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&["--source", "tidak-ada-source"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("tidak dikenal"), "{err}");
    assert!(err.contains("samehadaku"), "{err}");
}

#[test]
fn config_show_prints_effective_toml() {
    let sandbox = Sandbox::new();

    let output = sandbox.run(&["config", "show"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("config_version"), "{text}");
    assert!(text.contains("[playback]"), "{text}");
    assert!(text.contains("default_resolution = \"720p\""), "{text}");
}

#[test]
fn missing_mock_directory_fails_cleanly() {
    let sandbox = Sandbox::new();
    let missing = sandbox.dir.path().join("tidak-ada");

    let output = sandbox.run(&["--mock", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("--mock"), "{}", stderr(&output));
}

#[test]
fn config_is_reloaded_on_second_run() {
    let sandbox = Sandbox::new();

    // Run pertama: config default dibuat.
    let first = sandbox.run(&["--no-tui"]);
    assert!(first.status.success(), "{}", stderr(&first));

    // Ubah config (simulasi user mengedit file), lalu pastikan terbaca ulang.
    let mut config = std::fs::read_to_string(sandbox.config_file()).unwrap();
    config = config.replace(
        "default_resolution = \"720p\"",
        "default_resolution = \"480p\"",
    );
    std::fs::write(sandbox.config_file(), config).unwrap();

    let second = sandbox.run(&["--no-tui"]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(
        stdout(&second).contains("480p"),
        "config baru harus terbaca: {}",
        stdout(&second)
    );
}
