use super::logger::{LogEntry, LogType};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

pub struct ProcessRunner;

impl ProcessRunner {
    /// Spawns a command inside the specified working directory with injected environment variables.
    /// Captures stdout and stderr in background threads and transmits log entries via `log_tx`.
    pub fn spawn(
        work_dir: &str,
        command_str: &str,
        env_vars: &HashMap<String, String>,
        log_tx: Sender<LogEntry>,
    ) -> Result<Child, String> {
        let trimmed_cmd = command_str.trim();
        if trimmed_cmd.is_empty() {
            return Err("Command cannot be empty".to_string());
        }

        let dir = Path::new(work_dir);
        if !work_dir.is_empty() && !dir.exists() {
            return Err(format!("Working directory does not exist: {}", work_dir));
        }

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", trimmed_cmd]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", trimmed_cmd]);
            c
        };

        if !work_dir.is_empty() {
            cmd.current_dir(dir);
        }

        // Inject environment variables
        for (key, val) in env_vars {
            cmd.env(key, val);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let _ = log_tx.send(LogEntry::new(
            LogType::SystemInfo,
            format!("Process started with PID {}", child.id()),
        ));

        // Read stdout in background thread
        if let Some(stdout) = child.stdout.take() {
            let tx = log_tx.clone();
            thread::Builder::new()
                .name("stdout-reader".to_string())
                .spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        let _ = tx.send(LogEntry::new(LogType::Stdout, line));
                    }
                })
                .map_err(|e| format!("Failed to spawn stdout reader thread: {}", e))?;
        }

        // Read stderr in background thread
        if let Some(stderr) = child.stderr.take() {
            let tx = log_tx;
            thread::Builder::new()
                .name("stderr-reader".to_string())
                .spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().flatten() {
                        let _ = tx.send(LogEntry::new(LogType::Stderr, line));
                    }
                })
                .map_err(|e| format!("Failed to spawn stderr reader thread: {}", e))?;
        }

        Ok(child)
    }

    /// Terminates a process and its child tree.
    pub fn kill_process_tree(child: &mut Child) {
        let pid = child.id();

        #[cfg(target_os = "windows")]
        {
            // /F = Forcefully terminate, /T = Terminate tree (all child processes)
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = child.kill();
        }

        let _ = child.wait();
    }
}

