use crate::config::{AppStatus, DeploymentConfig, LogDatabase, Storage};
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
    log_db: LogDatabase,
}

impl DeploymentManager {
    pub fn new() -> Self {
        let storage = Storage::new();
        let log_db = LogDatabase::new();
        Self::with_storage_and_db(storage, log_db)
    }

    pub fn with_storage_and_db(storage: Storage, log_db: LogDatabase) -> Self {
        let loaded = storage.load();

        let mut deployments = HashMap::new();
        let mut statuses = HashMap::new();
        let mut logs = HashMap::new();

        for d in loaded {
            statuses.insert(d.id.clone(), AppStatus::Stopped);
            let mut buf = LogBuffer::new(MAX_LOG_LINES);
            if d.persist_logs {
                if let Ok(past_logs) = log_db.fetch_recent_logs(&d.id, MAX_LOG_LINES) {
                    for e in past_logs {
                        buf.push(e);
                    }
                }
            }
            logs.insert(d.id.clone(), buf);
            deployments.insert(d.id.clone(), d);
        }

        Self {
            deployments,
            statuses,
            processes: HashMap::new(),
            logs,
            log_receivers: HashMap::new(),
            storage,
            log_db,
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
            let mut buf = LogBuffer::new(MAX_LOG_LINES);
            if config.persist_logs {
                if let Ok(past_logs) = self.log_db.fetch_recent_logs(&id, MAX_LOG_LINES) {
                    for e in past_logs {
                        buf.push(e);
                    }
                }
            }
            self.logs.insert(id.clone(), buf);
            self.statuses.insert(id.clone(), AppStatus::Stopped);
        }
        if config.persist_logs {
            if let Err(e) = self.log_db.create_table(&id) {
                eprintln!("Failed to create SQLite table for deployment {}: {}", id, e);
            }
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
        if let Err(e) = self.log_db.delete_table(id) {
            eprintln!("Failed to delete SQLite table for deployment {}: {}", id, e);
        }
        self.save_to_storage()
    }

    pub fn log_db(&self) -> &LogDatabase {
        &self.log_db
    }

    pub fn push_log(&mut self, id: &str, entry: LogEntry) {
        if let Some(buf) = self.logs.get_mut(id) {
            buf.push(entry.clone());
        }
        if let Some(cfg) = self.deployments.get(id) {
            if cfg.persist_logs {
                if let Err(e) = self.log_db.insert_logs(id, &[entry]) {
                    eprintln!("Failed to persist log in SQLite for {}: {}", id, e);
                }
            }
        }
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
                self.push_log(id, LogEntry::new(LogType::SystemError, format!("Failed to start: {}", e)));
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
            self.push_log(id, LogEntry::new(LogType::SystemInfo, "Process stopped by user"));
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
                if let Some(cfg) = self.deployments.get(id) {
                    if cfg.persist_logs {
                        if let Err(e) = self.log_db.insert_logs(id, &entries) {
                            eprintln!("Failed to persist logs in SQLite for {}: {}", id, e);
                        }
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

            let exit_entry = LogEntry::new(
                if status.success() { LogType::SystemInfo } else { LogType::SystemError },
                format!("Process exited with status: {}", status),
            );
            self.push_log(&id, exit_entry);

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
            persist_logs: false,
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

    #[test]
    fn test_manager_sqlite_log_persistence_and_deletion() {
        let db = LogDatabase::in_memory().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("liveship_mgr_test_{}", uuid::Uuid::new_v4()));
        let storage = Storage::with_path(temp_dir.join("deployments.json"));
        let mut manager = DeploymentManager::with_storage_and_db(storage, db.clone());

        let id = format!("test-sqlite-{}", uuid::Uuid::new_v4());
        let config = DeploymentConfig {
            id: id.clone(),
            name: "SQLite Persist App".to_string(),
            app_type: AppType::Custom,
            work_dir: ".".to_string(),
            command: "echo test".to_string(),
            env_vars: HashMap::new(),
            auto_restart: false,
            persist_logs: true,
            port: None,
            created_at: chrono::Local::now().to_rfc3339(),
        };

        // Add deployment with persist_logs = true -> table created
        assert!(manager.add_or_update(config).is_ok());
        assert!(db.table_exists(&id));

        // Manually push log entries through manager
        manager.push_log(&id, LogEntry::new(LogType::Stdout, "Persisted stdout message"));
        manager.push_log(&id, LogEntry::new(LogType::Stderr, "Persisted stderr warning"));

        // Verify persisted in SQLite
        let logs = db.fetch_recent_logs(&id, 10).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "Persisted stdout message");
        assert_eq!(logs[1].message, "Persisted stderr warning");

        // Delete deployment -> table is deleted
        assert!(manager.delete_deployment(&id).is_ok());
        assert!(!db.table_exists(&id));

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}


