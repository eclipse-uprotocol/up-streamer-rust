use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

#[derive(Debug)]
pub struct CommandOutcome {
    pub command: String,
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub name: String,
    pub workdir: PathBuf,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub log_file_name: String,
}

#[derive(Debug)]
pub struct ManagedProcess {
    pub name: String,
    pub pid: i32,
    pub process_group_id: i32,
    pub command_line: String,
    pub workdir: PathBuf,
    pub log_path: PathBuf,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub exit_status_code: Option<i32>,
    child: Child,
}

pub async fn run_shell_command(
    repo_root: &Path,
    workdir: &Path,
    command: &str,
    no_bootstrap: bool,
) -> Result<CommandOutcome> {
    let full_command = with_optional_bootstrap(repo_root, command, no_bootstrap);
    let output = Command::new("bash")
        .arg("-lc")
        .arg(&full_command)
        .current_dir(workdir)
        .output()
        .await
        .with_context(|| format!("failed to execute shell command: {full_command}"))?;

    Ok(CommandOutcome {
        command: full_command,
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn shell_escape(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

impl ManagedProcess {
    pub async fn spawn(
        spec: ProcessSpec,
        repo_root: &Path,
        artifacts_dir: &Path,
        no_bootstrap: bool,
    ) -> Result<Self> {
        let log_path = artifacts_dir.join(&spec.log_file_name);
        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("unable to open log file {}", log_path.display()))?;
        let stderr_file = stdout_file
            .try_clone()
            .with_context(|| format!("unable to clone log file {}", log_path.display()))?;

        let exec_command = render_exec_command(&spec.executable, &spec.args);
        let env_prefix = render_env_prefix(&spec.env);
        let shell_body = if env_prefix.is_empty() {
            exec_command
        } else {
            format!("{env_prefix} && {exec_command}")
        };
        let full_command = with_optional_bootstrap(repo_root, &shell_body, no_bootstrap);

        let mut command = Command::new("bash");
        command
            .arg("-lc")
            .arg(&full_command)
            .current_dir(&spec.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = command.spawn().with_context(|| {
            format!(
                "unable to spawn process '{}' with command '{}': in {}",
                spec.name,
                full_command,
                spec.workdir.display()
            )
        })?;

        let pid = child
            .id()
            .ok_or_else(|| anyhow!("spawned process '{}' has no pid", spec.name))?
            as i32;

        Ok(Self {
            name: spec.name,
            pid,
            process_group_id: pid,
            command_line: full_command,
            workdir: spec.workdir,
            log_path,
            started_at: Utc::now(),
            ended_at: None,
            exit_status_code: None,
            child,
        })
    }

    pub fn has_exited(&self) -> bool {
        self.ended_at.is_some()
    }

    pub async fn refresh_exit_status(&mut self) -> Result<()> {
        if self.exit_status_code.is_some() {
            return Ok(());
        }

        if let Some(status) = self.child.try_wait().with_context(|| {
            format!(
                "unable to query process status for '{}'(pid={})",
                self.name, self.pid
            )
        })? {
            self.exit_status_code = status.code();
            self.ended_at = Some(Utc::now());
        }

        Ok(())
    }

    pub async fn wait_with_timeout(&mut self, timeout: Duration) -> Result<bool> {
        self.refresh_exit_status().await?;
        if self.exit_status_code.is_some() {
            return Ok(true);
        }

        let wait_result = tokio::time::timeout(timeout, self.child.wait()).await;
        match wait_result {
            Ok(wait_outcome) => {
                let status = wait_outcome.with_context(|| {
                    format!(
                        "unable to wait on process '{}'(pid={})",
                        self.name, self.pid
                    )
                })?;
                self.exit_status_code = status.code();
                self.ended_at = Some(Utc::now());
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    pub async fn terminate_gracefully(
        &mut self,
        sigint_grace: Duration,
        sigterm_grace: Duration,
    ) -> Result<()> {
        self.refresh_exit_status().await?;
        if self.exit_status_code.is_some() {
            return Ok(());
        }

        self.signal_process_group(libc::SIGINT)?;
        if self.wait_with_timeout(sigint_grace).await? {
            return Ok(());
        }

        self.signal_process_group(libc::SIGTERM)?;
        if self.wait_with_timeout(sigterm_grace).await? {
            return Ok(());
        }

        self.refresh_exit_status().await?;
        if self.exit_status_code.is_none() {
            return Err(anyhow!(
                "process '{}' (pid={}, pgid={}) did not terminate after SIGINT/SIGTERM grace windows",
                self.name,
                self.pid,
                self.process_group_id
            ));
        }

        Ok(())
    }

    fn signal_process_group(&self, signal: i32) -> Result<()> {
        let target_pgid = -self.process_group_id;
        let rc = unsafe { libc::kill(target_pgid, signal) };
        if rc == 0 {
            return Ok(());
        }

        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }

        Err(anyhow!(
            "unable to send signal {} to process group {}: {}",
            signal,
            self.process_group_id,
            error
        ))
    }
}

fn render_env_prefix(environment: &[(String, String)]) -> String {
    environment
        .iter()
        .map(|(key, value)| format!("export {key}={}", shell_escape(value)))
        .collect::<Vec<String>>()
        .join(" && ")
}

fn render_exec_command(executable: &Path, args: &[String]) -> String {
    let mut tokens = Vec::with_capacity(args.len() + 1);
    tokens.push(shell_escape(executable.to_string_lossy().as_ref()));
    for arg in args {
        tokens.push(shell_escape(arg));
    }
    format!("exec {}", tokens.join(" "))
}

fn with_optional_bootstrap(repo_root: &Path, command: &str, no_bootstrap: bool) -> String {
    if no_bootstrap {
        return command.to_string();
    }

    let bootstrap_script = repo_root.join("build").join("envsetup.sh");
    format!(
        "source {} highest >/dev/null 2>&1 && {command}",
        shell_escape(bootstrap_script.to_string_lossy().as_ref())
    )
}
