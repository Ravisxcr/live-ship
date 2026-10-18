use super::deployment::DeploymentConfig;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Storage {
    file_path: PathBuf,
}

impl Storage {
    pub fn new() -> Self {
        let file_path = Self::get_default_path();
        Self { file_path }
    }

    pub fn with_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            file_path: path.as_ref().to_path_buf(),
        }
    }

    fn get_default_path() -> PathBuf {
        if let Some(mut dir) = dirs_fallback() {
            dir.push("liveship");
            let _ = fs::create_dir_all(&dir);
            dir.push("deployments.json");
            dir
        } else {
            PathBuf::from("deployments.json")
        }
    }

    pub fn load(&self) -> Vec<DeploymentConfig> {
        if !self.file_path.exists() {
            return Vec::new();
        }

        match fs::read_to_string(&self.file_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(e) => {
                eprintln!("Failed to read deployments file: {}", e);
                Vec::new()
            }
        }
    }

    pub fn save(&self, deployments: &[DeploymentConfig]) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let json = serde_json::to_string_pretty(deployments)
            .map_err(|e| format!("Serialization error: {}", e))?;

        fs::write(&self.file_path, json).map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }
}

fn dirs_fallback() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppType;

    #[test]
    fn test_storage_save_and_load() {
        let temp_dir = std::env::temp_dir().join(format!("liveship_test_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join("deployments.json");
        let storage = Storage::with_path(&file_path);

        let config = DeploymentConfig {
            id: "test-1".to_string(),
            name: "Test FastAPI App".to_string(),
            app_type: AppType::FastApi,
            work_dir: ".".to_string(),
            command: "python app.py".to_string(),
            env_vars: std::collections::HashMap::new(),
            auto_restart: false,
            port: Some(8000),
            created_at: "2026-10-18T00:00:00Z".to_string(),
        };

        let res = storage.save(&[config.clone()]);
        assert!(res.is_ok());

        let loaded = storage.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test FastAPI App");

        let _ = fs::remove_dir_all(temp_dir);
    }
}

