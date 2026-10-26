//! Bounded Linux Pandoc execution over an owned, temporary input snapshot.
use crate::illustration::AssetFile;
use std::{
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Debug)]
pub enum FilterStage {
    Lua(PathBuf),
    /// Standard executable JSON filter: receives writer target `json` as argv[1].
    Json(PathBuf),
    /// Explicit nonstandard preparation command; `--source SNAPSHOT` is appended.
    Preparation {
        program: PathBuf,
        args: Vec<String>,
    },
}
#[derive(Clone, Debug)]
pub struct PandocConfig {
    pub executable: PathBuf,
    pub stages: Vec<FilterStage>,
    pub timeout: Duration,
    pub output_limit: usize,
}
impl Default for PandocConfig {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("/usr/bin/pandoc"),
            stages: Vec::new(),
            timeout: Duration::from_secs(15),
            output_limit: 16 * 1024 * 1024,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("invalid processing snapshot: {0}")]
    Snapshot(String),
    #[error("processing cancelled")]
    Cancelled,
    #[error("processing deadline exceeded")]
    Timeout,
    #[error("processing output quota exceeded")]
    OutputLimit,
    #[error("processing I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("processing command failed: {0}")]
    Command(String),
    #[error("invalid Pandoc JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("illustration preparation: {0}")]
    Illustration(#[from] crate::illustration::PreparationError),
    #[error("external Pandoc processing is supported on Linux only")]
    Unsupported,
}

/// Accept only exact portable relative snapshot keys, without aliases or traversal.
pub(crate) fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', '\0', ':'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && Path::new(value)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}

pub fn run(
    source: &str,
    logical_path: &str,
    resources: &[AssetFile],
    config: &PandocConfig,
    cancel: Arc<AtomicBool>,
) -> Result<serde_json::Value, ProcessingError> {
    if source.len() > 1024 * 1024 || resources.len() > 128 {
        return Err(ProcessingError::Snapshot("source/file-count quota".into()));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for resource in resources {
        total = total.saturating_add(resource.bytes.len());
        if !safe_path(&resource.relative_path)
            || resource.relative_path == logical_path
            || !seen.insert(&resource.relative_path)
            || resource.bytes.len() > 64 * 1024 * 1024
            || total > 80 * 1024 * 1024
        {
            return Err(ProcessingError::Snapshot(
                "unsafe resource key or resource quota".into(),
            ));
        }
    }
    #[cfg(target_os = "linux")]
    {
        linux::run(source, logical_path, resources, config, cancel)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (source, logical_path, resources, config, cancel);
        Err(ProcessingError::Unsupported)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        os::unix::{fs::DirBuilderExt, process::CommandExt},
        process::{Command, Stdio},
        thread,
        time::Instant,
    };
    struct SnapshotDir(PathBuf);
    impl Drop for SnapshotDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn read_bounded(
        mut stream: impl Read,
        limit: usize,
        exceeded: Arc<AtomicBool>,
    ) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                return Ok(data);
            }
            if n > limit.saturating_sub(data.len()) {
                exceeded.store(true, Ordering::Release);
                return Ok(data);
            }
            data.extend_from_slice(&buf[..n]);
        }
    }
    fn execute(
        mut command: Command,
        input: Vec<u8>,
        deadline: Instant,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, ProcessingError> {
        if cancel.load(Ordering::Acquire) {
            return Err(ProcessingError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(ProcessingError::Timeout);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn()?;
        let pid = child.id() as i32;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let exceeded = Arc::new(AtomicBool::new(false));
        let out_flag = exceeded.clone();
        let err_flag = exceeded.clone();
        let writer = thread::spawn(move || stdin.write_all(&input));
        let output = thread::spawn(move || read_bounded(stdout, limit, out_flag));
        let errors = thread::spawn(move || read_bounded(stderr, limit.min(64 * 1024), err_flag));
        let mut failure = None;
        let status = loop {
            if cancel.load(Ordering::Acquire) {
                failure = Some(ProcessingError::Cancelled);
            } else if exceeded.load(Ordering::Acquire) {
                failure = Some(ProcessingError::OutputLimit);
            } else if Instant::now() >= deadline {
                failure = Some(ProcessingError::Timeout);
            }
            if failure.is_some() {
                break None;
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    failure = Some(error.into());
                    break None;
                }
            }
        };
        // Kill the whole isolated process group, including descendants keeping pipes open.
        // SAFETY: negative PID addresses the child's new process group, not our group.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        let waited = child.wait();
        let written = writer
            .join()
            .map_err(|_| ProcessingError::Command("stdin worker panicked".into()))?;
        let stdout = output
            .join()
            .map_err(|_| ProcessingError::Command("stdout worker panicked".into()))??;
        let stderr = errors
            .join()
            .map_err(|_| ProcessingError::Command("stderr worker panicked".into()))??;
        if let Some(error) = failure {
            return Err(error);
        }
        if exceeded.load(Ordering::Acquire) {
            return Err(ProcessingError::OutputLimit);
        }
        let status = status
            .or_else(|| waited.ok())
            .ok_or_else(|| ProcessingError::Command("missing exit status".into()))?;
        if !status.success() {
            return Err(ProcessingError::Command(format!(
                "{status}: {}",
                String::from_utf8_lossy(&stderr)
            )));
        }
        written?;
        Ok(stdout)
    }
    pub(super) fn run(
        source: &str,
        logical_path: &str,
        resources: &[AssetFile],
        config: &PandocConfig,
        cancel: Arc<AtomicBool>,
    ) -> Result<serde_json::Value, ProcessingError> {
        if !safe_path(logical_path) {
            return Err(ProcessingError::Snapshot("unsafe source path".into()));
        }
        if !config.executable.is_absolute()
            || config.output_limit == 0
            || config.output_limit > 64 * 1024 * 1024
            || config.timeout.is_zero()
            || config.timeout > Duration::from_secs(120)
            || config.stages.len() > 16
        {
            return Err(ProcessingError::Snapshot(
                "invalid executable or processing limits".into(),
            ));
        }
        let root =
            std::env::temp_dir().join(format!("tanglewood-document-{}", uuid::Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&root)?;
        let snapshot = SnapshotDir(root);
        let source_path = snapshot.0.join(logical_path);
        if let Some(parent) = source_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&source_path, source)?;
        for asset in resources {
            if !safe_path(&asset.relative_path) || asset.relative_path == logical_path {
                return Err(ProcessingError::Snapshot(
                    "unsafe or conflicting resource path".into(),
                ));
            }
            let path = snapshot.0.join(&asset.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .write_all(&asset.bytes)?;
        }
        let deadline = Instant::now() + config.timeout;
        let mut reader = Command::new(&config.executable);
        reader
            .current_dir(&snapshot.0)
            .args(["--from=markdown", "--to=json"])
            .arg(&source_path);
        let mut json = execute(reader, Vec::new(), deadline, config.output_limit, &cancel)?;
        for stage in &config.stages {
            let mut command = match stage {
                FilterStage::Lua(path) => {
                    if !path.is_absolute() {
                        return Err(ProcessingError::Snapshot(
                            "Lua path must be absolute".into(),
                        ));
                    }
                    let mut c = Command::new(&config.executable);
                    c.args(["--from=json", "--to=json", "--lua-filter"])
                        .arg(path);
                    c
                }
                FilterStage::Json(path) => {
                    if !path.is_absolute() {
                        return Err(ProcessingError::Snapshot(
                            "JSON filter path must be absolute".into(),
                        ));
                    }
                    let mut c = Command::new(path);
                    c.arg("json");
                    c
                }
                FilterStage::Preparation { program, args } => {
                    if !program.is_absolute() {
                        return Err(ProcessingError::Snapshot(
                            "preparation program must be absolute".into(),
                        ));
                    }
                    let mut c = Command::new(program);
                    c.args(args).arg("--source").arg(&source_path);
                    c
                }
            };
            command.current_dir(&snapshot.0);
            json = execute(command, json, deadline, config.output_limit, &cancel)?;
            validate_json(&json)?;
        }
        validate_json(&json)
    }
    #[cfg(test)]
    mod process_tests {
        use super::*;
        #[test]
        fn deadline_output_and_cancel_are_bounded() {
            let mut command = Command::new("/usr/bin/python3");
            command.args(["-c", "import time; time.sleep(10)"]);
            assert!(matches!(
                execute(
                    command,
                    vec![],
                    Instant::now() + Duration::from_millis(80),
                    1024,
                    &AtomicBool::new(false)
                ),
                Err(ProcessingError::Timeout)
            ));
            let mut command = Command::new("/usr/bin/python3");
            command.args(["-c", "import sys; sys.stdout.write('x'*10000)"]);
            assert!(matches!(
                execute(
                    command,
                    vec![],
                    Instant::now() + Duration::from_secs(2),
                    128,
                    &AtomicBool::new(false)
                ),
                Err(ProcessingError::OutputLimit)
            ));
            // A successfully exiting parent must not leave a descendant holding capture pipes.
            let mut command = Command::new("/usr/bin/python3");
            command.args(["-c", "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import time; time.sleep(10)']); print('done')"]);
            let started = Instant::now();
            assert_eq!(
                execute(
                    command,
                    vec![],
                    started + Duration::from_secs(2),
                    128,
                    &AtomicBool::new(false)
                )
                .unwrap(),
                b"done\n"
            );
            assert!(started.elapsed() < Duration::from_secs(2));
            let command = Command::new("/usr/bin/python3");
            assert!(matches!(
                execute(
                    command,
                    vec![],
                    Instant::now() + Duration::from_secs(2),
                    128,
                    &AtomicBool::new(true)
                ),
                Err(ProcessingError::Cancelled)
            ));
        }
    }
    fn validate_json(bytes: &[u8]) -> Result<serde_json::Value, ProcessingError> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        if !value
            .get("pandoc-api-version")
            .is_some_and(|v| v.is_array())
            || !value.get("blocks").is_some_and(|v| v.is_array())
            || !value.get("meta").is_some_and(|v| v.is_object())
        {
            return Err(ProcessingError::Snapshot(
                "missing Pandoc JSON envelope".into(),
            ));
        }
        Ok(value)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires real Pandoc and explicitly selected pinned illustration-md checkout"]
    fn real_ordered_fixture() {
        let upstream = std::env::var("TANGLEWOOD_ILLUSTRATION_MD").expect("select pinned checkout");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/fixtures/mvp-2");
        let source = std::fs::read_to_string(fixture.join("source.md")).unwrap();
        let paths = [
            "assets/fig-add-inputs/figure.svg",
            "assets/fig-add-inputs/illustration.json",
            "assets/fig-choosing-a-result/strip.png",
            "assets/fig-choosing-a-result/comic.json",
        ];
        let mut resources: Vec<_> = paths
            .iter()
            .map(|path| AssetFile {
                relative_path: (*path).into(),
                bytes: std::fs::read(fixture.join(path)).unwrap(),
            })
            .collect();
        let config = PandocConfig {
            stages: vec![
                FilterStage::Lua(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("docs/plan/examples/callout.lua"),
                ),
                FilterStage::Preparation {
                    program: PathBuf::from("/usr/bin/python3"),
                    args: vec![
                        "-c".into(),
                        format!(
                            "import sys,runpy; sys.path.insert(0, {}); runpy.run_module('illustration_md.prepare',run_name='__main__')",
                            serde_json::to_string(&upstream).unwrap()
                        ),
                    ],
                },
            ],
            ..PandocConfig::default()
        };
        let output = run(
            &source,
            "source.md",
            &resources,
            &config,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let blocks = output["blocks"].as_array().unwrap();
        assert_eq!(
            blocks.iter().filter(|node| node["t"] == "Figure").count(),
            2
        );
        assert!(blocks.iter().any(|node| node["t"] == "Div"
            && node["c"][0][0] == "callout-proof"
            && node["c"][1][0]["t"] == "BlockQuote"));
        resources.remove(0);
        assert!(
            run(
                &source,
                "source.md",
                &resources,
                &config,
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(fixture.join("source.md")).unwrap(),
            source
        );
    }
}
