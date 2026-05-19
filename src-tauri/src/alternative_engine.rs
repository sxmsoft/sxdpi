use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use thiserror::Error;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const ALTERNATIVE_EXE_NAME: &str = "sxdpialternative.exe";

#[derive(Error, Debug)]
pub enum AlternativeEngineError {
    #[error("SxDPI Alternative executable not found. Put sxdpialternative.exe under x86_64/ or x86/ next to SxDPI.")]
    ExecutableNotFound,

    #[error("SxDPI Alternative could not be started: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("SxDPI Alternative exited immediately with code {0:?}. It may need administrator privileges.")]
    ExitedImmediately(Option<i32>),
}

pub struct AlternativeProcess {
    child: Child,
}

impl AlternativeProcess {
    pub fn start() -> Result<Self, AlternativeEngineError> {
        let exe = find_alternative_exe().ok_or(AlternativeEngineError::ExecutableNotFound)?;
        let workdir = exe.parent().unwrap_or_else(|| Path::new("."));

        let mut command = Command::new(&exe);
        command
            .current_dir(workdir)
            .args([
                "-5",
                "--set-ttl",
                "5",
                "--dns-addr",
                "77.88.8.8",
                "--dns-port",
                "1253",
                "--dnsv6-addr",
                "2a02:6b8::feed:0ff",
                "--dnsv6-port",
                "1253",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command.spawn()?;
        std::thread::sleep(Duration::from_millis(200));

        if let Some(status) = child.try_wait()? {
            return Err(AlternativeEngineError::ExitedImmediately(status.code()));
        }

        log::info!("SxDPI Alternative started: {}", exe.display());
        Ok(Self { child })
    }

    pub fn stop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            Err(e) => log::debug!("SxDPI Alternative process status failed: {}", e),
        }
    }
}

impl Drop for AlternativeProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_alternative_exe() -> Option<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            push_unique(&mut roots, parent.to_path_buf());
            if let Some(parent_parent) = parent.parent() {
                push_unique(&mut roots, parent_parent.to_path_buf());
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        push_unique(&mut roots, cwd.clone());
        if let Some(parent) = cwd.parent() {
            push_unique(&mut roots, parent.to_path_buf());
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    push_unique(&mut roots, manifest_dir.clone());
    if let Some(parent) = manifest_dir.parent() {
        push_unique(&mut roots, parent.to_path_buf());
    }

    for root in roots {
        for rel in [
            Path::new("x86_64").join(ALTERNATIVE_EXE_NAME),
            Path::new("x86").join(ALTERNATIVE_EXE_NAME),
            Path::new("resources")
                .join("x86_64")
                .join(ALTERNATIVE_EXE_NAME),
            Path::new("resources").join("x86").join(ALTERNATIVE_EXE_NAME),
            Path::new("resources").join(ALTERNATIVE_EXE_NAME),
            PathBuf::from(ALTERNATIVE_EXE_NAME),
        ] {
            let candidate = root.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn push_unique(items: &mut Vec<PathBuf>, item: PathBuf) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}
