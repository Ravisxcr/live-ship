use crate::config::{AppStatus, DeploymentConfig, Storage};
use crate::utils::{LogBuffer, LogEntry, LogType, ProcessRunner};
use std::collections::HashMap;
use std::process::Child;
use std::sync::mpsc::{channel, Receiver};

const MAX_LOG_LINES: usize = 2000;

pub struct DeploymentManager {
    deployments: HashMap<String, DeploymentConfig>,
    statuses: HashMap<String, AppStatus>,
    processes: HashMap<String, Child>,
    logs: HashMap<String, LogBuffer>,
    log_receivers: HashMap<String, Receiver<LogEntry>>,
    storage: Storage,
}

impl DeploymentManager {
    pub fn new() -> Self {
        let storage = Storage::new();
        let loaded = storage.load();

        let mut deployments = HashMap::new();
        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();

        for d in loaded {
            statuses.insert(d.id.clone(), AppStatus::Stopped);
            logs.insert(d.id.clone(), LogBuffer::new(MAX_LOG_LINES));
            deployments.insert(d.id.clone(), d);
        }

        Self {
            deployments,
            statuses,
            processes: HashMap::new(),
            logs,
            log_receivers: HashMap::new(),
            storage,
        }
    }

    pub fn save_to_storage(&self) -> Result<(), String> {
        let list: Vec<DeploymentConfig> = self.deployments.values().cloned().collect();
        self.storage.save(&list)
    }

    pub fn all_deployments(&self) -> Vec<DeploymentConfig> {
        let mut list: Vec<DeploymentConfig> = self.deployments.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn get_deployment(&self, id: &str) -> Option<&DeploymentConfig> {
        self.deployments.get(id)
    }

    pub fn get_status(&self, id: &str) -> AppStatus {
        self.statuses.get(id).cloned().unwrap_or(AppStatus::Stopped)
    }

    pub fn add_or_update(&mut self, config: DeploymentConfig) -> Result<(), String> {
        let id = config.id.clone();
        if !self.logs.contains_key(&id) {
            self.logs.insert(id.clone(), LogBuffer::new(MAX_LOG_LINES));
            self.statuses.insert(id.clone(), AppStatus::Stopped);
        }
        self.deployments.insert(id, config);
        self.save_to_storage()
    }

    pub fn delete_deployment(&mut self, id: &str) -> Result<(), String> {
        if self.is_running(id) {
            let _ = self.stop(id);
        }
        self.deployments.remove(id);
        self.statuses.remove(id);
        self.logs.remove(id);
        self.log_receivers.remove(id);
        self.save_to_storage()
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.processes.contains_key(id)
    }

    pub fn start(&mut self, id: &str) -> Result<(), String> {
        if self.is_running(id) {
            return Err("Deployment is already running".to_string());
        }

        let config = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("Deployment not found: {}", id))?
            .clone();

        let (tx, rx) = channel::<LogEntry>();
        self.statuses.insert(id.to_string(), AppStatus::Starting);

        let child = match ProcessRunner::spawn(&config.work_dir, &config.command, &config.env_vars, tx) {
            Ok(c) => c,
            Err(e) => {
                self.statuses.insert(id.to_string(), AppStatus::Error(e.clone()));
                if let Some(buf) = self.logs.get_mut(id) {
                    buf.push(LogEntry::new(LogType::SystemError, format!("Failed to start: {}", e)));
                }
                return Err(e);
            }
        };

        self.processes.insert(id.to_string(), child);
        self.log_receivers.insert(id.to_string(), rx);
        self.statuses.insert(id.to_string(), AppStatus::Running);

        Ok(())
    }

    pub fn stop(&mut self, id: &str) -> Result<(), String> {
        if let Some(mut child) = self.processes.remove(id) {
            ProcessRunner::kill_process_tree(&mut child);
            self.statuses.insert(id.to_string(), AppStatus::Stopped);
            if let Some(buf) = self.logs.get_mut(id) {
                buf.push(LogEntry::new(LogType::SystemInfo, "Process stopped by user"));
            }
            Ok(())
        } else {
            self.statuses.insert(id.to_string(), AppStatus::Stopped);
            Ok(())
        }
    }

    pub fn restart(&mut self, id: &str) -> Result<(), String> {
        let _ = self.stop(id);
        self.start(id)
    }

    pub fn clear_logs(&mut self, id: &str) {
        if let Some(buf) = self.logs.get_mut(id) {
            buf.clear();
        }
    }

    pub fn get_logs(&self, id: &str) -> Vec<LogEntry> {
        self.logs
            .get(id)
            .map(|b| b.entries())
            .unwrap_or_default()
    }

    /// Periodic tick function to drain logs from background streams and check exit statuses.
    /// Returns a map of deployment ID to list of newly received log entries.
    pub fn tick(&mut self) -> HashMap<String, Vec<LogEntry>> {
        let mut new_logs = HashMap::new();

        // 1. Drain logs
        for (id, rx) in &self.log_receivers {
            let mut entries = Vec::new();
            while let Ok(entry) = rx.try_recv() {
                entries.push(entry);
            }

            if !entries.is_empty() {
                if let Some(buf) = self.logs.get_mut(id) {
                    for e in &entries {
                        buf.push(e.clone());
                    }
                }
                new_logs.insert(id.clone(), entries);
            }
        }

        // 2. Check process status
        let mut finished = Vec::new();
        for (id, child) in &mut self.processes {
            match child.try_wait() {
                Ok(Some(status)) => {
                    finished.push((id.clone(), status));
                }
                Ok(None) => {
                    // Still running
                }
                Err(e) => {
                    finished.push((
                        id.clone(),
                        std::process::ExitStatus::default(), // fallback
                    ));
                    eprintln!("Error waiting on child {}: {}", id, e);
                }
            }
        }

        for (id, status) in finished {
            self.processes.remove(&id);
            self.log_receivers.remove(&id);

            let exit_code = status.code();
            let new_status = if status.success() {
                AppStatus::Stopped
            } else {
                AppStatus::Error(format!("Exited with code {:?}", exit_code))
            };

            if let Some(buf) = self.logs.get_mut(&id) {
                buf.push(LogEntry::new(
                    if status.success() { LogType::SystemInfo } else { LogType::SystemError },
                    format!("Process exited with status: {}", status),
                ));
            }

            // Check auto-restart if configured
            let should_restart = self
                .deployments
                .get(&id)
                .map(|c| c.auto_restart)
                .unwrap_or(false);

            self.statuses.insert(id.clone(), new_status);

            if should_restart {
                let _ = self.start(&id);
            }
        }

        new_logs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppType;

    #[test]
    fn test_manager_crud_and_lifecycle() {
        let mut manager = DeploymentManager::new();
        let id = format!("test-{}", uuid::Uuid::new_v4());

        let config = DeploymentConfig {
            id: id.clone(),
            name: "Echo Test App".to_string(),
            app_type: AppType::Custom,
            work_dir: ".".to_string(),
            command: "echo hello".to_string(),
            env_vars: HashMap::new(),
            auto_restart: false,
            port: None,
            created_at: chrono::Local::now().to_rfc3339(),
        };

        // Add
        assert!(manager.add_or_update(config).is_ok());
        assert_eq!(manager.get_status(&id), AppStatus::Stopped);
        assert!(!manager.is_running(&id));

        // Start
        assert!(manager.start(&id).is_ok());
        assert!(manager.is_running(&id));

        // Stop
        assert!(manager.stop(&id).is_ok());
        assert!(!manager.is_running(&id));

        // Delete
        assert!(manager.delete_deployment(&id).is_ok());
        assert!(manager.get_deployment(&id).is_none());
    }
}


