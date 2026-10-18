use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppType {
    #[default]
    FastApi,
    GoBinary,
    ReactApp,
    Custom,
}

impl AppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppType::FastApi => "FastAPI (Python)",
            AppType::GoBinary => "Go Binary",
            AppType::ReactApp => "React App",
            AppType::Custom => "Custom Command",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "FastAPI (Python)" | "fastapi" | "FastApi" => AppType::FastApi,
            "Go Binary" | "gobinary" | "GoBinary" => AppType::GoBinary,
            "React App" | "react" | "ReactApp" => AppType::ReactApp,
            _ => AppType::Custom,
        }
    }

    pub fn default_command(&self) -> &'static str {
        match self {
            AppType::FastApi => "uvicorn main:app --reload --port 8000",
            AppType::GoBinary => "go run .",
            AppType::ReactApp => "npm run dev",
            AppType::Custom => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Error(String),
}

impl AppStatus {
    pub fn badge_text(&self) -> &'static str {
        match self {
            AppStatus::Stopped => "Stopped",
            AppStatus::Starting => "Starting",
            AppStatus::Running => "Running",
            AppStatus::Error(_) => "Error",
        }
    }

    pub fn hex_color(&self) -> &'static str {
        match self {
            AppStatus::Stopped => "#64748B", // slate-500
            AppStatus::Starting => "#F59E0B", // amber-500
            AppStatus::Running => "#10B981",  // emerald-500
            AppStatus::Error(_) => "#EF4444", // red-500
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, AppStatus::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub id: String,
    pub name: String,
    pub app_type: AppType,
    pub work_dir: String,
    pub command: String,
    pub env_vars: HashMap<String, String>,
    pub auto_restart: bool,
    #[serde(default)]
    pub persist_logs: bool,
    pub port: Option<u16>,
    pub created_at: String,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "New Deployment".to_string(),
            app_type: AppType::FastApi,
            work_dir: String::new(),
            command: "uvicorn main:app --reload".to_string(),
            env_vars: HashMap::new(),
            auto_restart: false,
            persist_logs: false,
            port: Some(8000),
            created_at: String::new(),
        }
    }
}

