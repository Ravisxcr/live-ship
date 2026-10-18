use crate::utils::{LogEntry, LogType};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Generates a safe SQLite table name for a given deployment ID.
/// Sanitizes all non-alphanumeric characters into underscores to guarantee a valid, SQL-safe identifier.
pub fn table_name_for_deployment(deployment_id: &str) -> String {
    let sanitized: String = deployment_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if sanitized.is_empty() {
        "logs_default".to_string()
    } else {
        format!("logs_{}", sanitized)
    }
}

#[derive(Clone)]
pub struct LogDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl LogDatabase {
    /// Connects to the default persistent SQLite database file located in app data (%APPDATA%/liveship/logs.db).
    pub fn new() -> Self {
        let path = Self::get_default_path();
        Self::with_path(path).unwrap_or_else(|e| {
            eprintln!(
                "Failed to open persistent SQLite database at {:?}: {}, falling back to in-memory",
                Self::get_default_path(),
                e
            );
            Self::in_memory().expect("Failed to initialize fallback in-memory SQLite database")
        })
    }

    /// Connects to a SQLite database at a specified file path.
    pub fn with_path<P: AsRef<Path>>(path: P) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        // Enable WAL mode and normal synchronous flag for high-performance concurrent writes
        let _ = conn.execute("PRAGMA journal_mode = WAL;", []);
        let _ = conn.execute("PRAGMA synchronous = NORMAL;", []);
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Initializes an in-memory SQLite database (primarily used for unit tests or fallback).
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn get_default_path() -> PathBuf {
        if let Some(mut dir) = dirs_fallback() {
            dir.push("liveship");
            let _ = std::fs::create_dir_all(&dir);
            dir.push("logs.db");
            dir
        } else {
            PathBuf::from("logs.db")
        }
    }

    /// Creates a dedicated log table for the given deployment ID if it does not already exist.
    pub fn create_table(&self, deployment_id: &str) -> Result<(), rusqlite::Error> {
        let table_name = table_name_for_deployment(deployment_id);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    log_type TEXT NOT NULL,
                    message TEXT NOT NULL,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                )",
                table_name
            ),
            [],
        )?;
        Ok(())
    }

    /// Drops the dedicated log table for the given deployment ID.
    /// This is invoked when the deployment is deleted.
    pub fn delete_table(&self, deployment_id: &str) -> Result<(), rusqlite::Error> {
        let table_name = table_name_for_deployment(deployment_id);
        let conn = self.conn.lock().unwrap();
        conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", table_name), [])?;
        Ok(())
    }

    /// Checks if a log table exists for the given deployment ID.
    pub fn table_exists(&self, deployment_id: &str) -> bool {
        let table_name = table_name_for_deployment(deployment_id);
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
            [&table_name],
            |_| Ok(true),
        )
        .unwrap_or(false)
    }

    /// Inserts a batch of log entries into the deployment's dedicated SQLite table.
    pub fn insert_logs(&self, deployment_id: &str, entries: &[LogEntry]) -> Result<(), rusqlite::Error> {
        if entries.is_empty() {
            return Ok(());
        }

        let table_name = table_name_for_deployment(deployment_id);
        let conn = self.conn.lock().unwrap();

        // Ensure table exists
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    log_type TEXT NOT NULL,
                    message TEXT NOT NULL,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                )",
                table_name
            ),
            [],
        )?;

        // Execute batch insertion within a transaction for speed and atomicity
        conn.execute("BEGIN TRANSACTION", [])?;
        {
            let mut stmt = conn.prepare(&format!(
                "INSERT INTO \"{}\" (timestamp, log_type, message) VALUES (?1, ?2, ?3)",
                table_name
            ))?;

            for entry in entries {
                stmt.execute(params![entry.timestamp, entry.log_type.as_str(), entry.message])?;
            }
        }
        conn.execute("COMMIT", [])?;

        Ok(())
    }

    /// Fetches the most recent log entries up to `limit` from the deployment's table in chronological order.
    pub fn fetch_recent_logs(&self, deployment_id: &str, limit: usize) -> Result<Vec<LogEntry>, rusqlite::Error> {
        let table_name = table_name_for_deployment(deployment_id);
        let conn = self.conn.lock().unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
                [&table_name],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !exists {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(&format!(
            "SELECT timestamp, log_type, message FROM \"{}\" ORDER BY id DESC LIMIT ?1",
            table_name
        ))?;

        let rows = stmt.query_map([limit as i64], |row| {
            let timestamp: String = row.get(0)?;
            let log_type_str: String = row.get(1)?;
            let message: String = row.get(2)?;
            Ok(LogEntry {
                timestamp,
                log_type: LogType::from_str(&log_type_str),
                message,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            if let Ok(entry) = row {
                entries.push(entry);
            }
        }
        // Since we fetched in descending order to get the most recent, reverse to restore chronological display
        entries.reverse();
        Ok(entries)
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

    #[test]
    fn test_log_database_lifecycle() {
        let db = LogDatabase::in_memory().expect("in memory db");
        let deployment_id = "test-deploy-123";

        assert!(!db.table_exists(deployment_id));

        // Create table
        assert!(db.create_table(deployment_id).is_ok());
        assert!(db.table_exists(deployment_id));

        // Insert logs
        let entries = vec![
            LogEntry::new(LogType::Stdout, "Application starting..."),
            LogEntry::new(LogType::Stderr, "Minor warning: port already checked"),
            LogEntry::new(LogType::SystemInfo, "Health probe 200 OK"),
        ];
        assert!(db.insert_logs(deployment_id, &entries).is_ok());

        // Fetch logs
        let fetched = db.fetch_recent_logs(deployment_id, 10).expect("fetch logs");
        assert_eq!(fetched.len(), 3);
        assert_eq!(fetched[0].message, "Application starting...");
        assert_eq!(fetched[0].log_type, LogType::Stdout);
        assert_eq!(fetched[1].message, "Minor warning: port already checked");
        assert_eq!(fetched[1].log_type, LogType::Stderr);
        assert_eq!(fetched[2].message, "Health probe 200 OK");
        assert_eq!(fetched[2].log_type, LogType::SystemInfo);

        // Delete table when deployment is deleted
        assert!(db.delete_table(deployment_id).is_ok());
        assert!(!db.table_exists(deployment_id));

        // Verify fetching from deleted table yields empty vec without error
        let empty_fetched = db.fetch_recent_logs(deployment_id, 10).expect("fetch empty");
        assert!(empty_fetched.is_empty());
    }

    #[test]
    fn test_table_name_sanitization() {
        assert_eq!(
            table_name_for_deployment("550e8400-e29b-41d4-a716-446655440000"),
            "logs_550e8400_e29b_41d4_a716_446655440000"
        );
        assert_eq!(
            table_name_for_deployment("my test app (v1)"),
            "logs_my_test_app__v1_"
        );
    }
}

