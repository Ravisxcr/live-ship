use crate::config::{AppStatus, DeploymentConfig};
use crate::utils::{LogBuffer, LogEntry};
use std::collections::HashMap;

#[derive(Default)]
pub struct AppState {
    pub deployments: HashMap<String, DeploymentConfig>,
    pub statuses: HashMap<String, AppStatus>,
    pub logs: HashMap<String, LogBuffer>,
    pub active_deployment_id: Option<String>,
    pub sidebar_collapsed: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            deployments: HashMap::new(),
            statuses: HashMap::new(),
            logs: HashMap::new(),
            active_deployment_id: None,
            sidebar_collapsed: false,
        }
    }

    pub fn get_status(&self, id: &str) -> AppStatus {
        self.statuses.get(id).cloned().unwrap_or(AppStatus::Stopped)
    }

    pub fn get_logs(&self, id: &str) -> Vec<LogEntry> {
        self.logs
            .get(id)
            .map(|buf| buf.entries())
            .unwrap_or_default()
    }
}

