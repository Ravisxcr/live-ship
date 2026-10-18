use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogType {
    Stdout,
    Stderr,
    SystemInfo,
    SystemError,
}

impl LogType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogType::Stdout => "STDOUT",
            LogType::Stderr => "STDERR",
            LogType::SystemInfo => "SYSTEM",
            LogType::SystemError => "SYS-ERR",
        }
    }

    pub fn color_hex(&self) -> &'static str {
        match self {
            LogType::Stdout => "#E2E8F0",     // slate-200
            LogType::Stderr => "#F87171",     // red-400
            LogType::SystemInfo => "#38BDF8", // sky-400
            LogType::SystemError => "#EF4444",// red-500
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "STDOUT" => LogType::Stdout,
            "STDERR" => LogType::Stderr,
            "SYSTEM" => LogType::SystemInfo,
            "SYS-ERR" => LogType::SystemError,
            _ => LogType::Stdout,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub log_type: LogType,
    pub message: String,
}

impl LogEntry {
    pub fn new(log_type: LogType, message: impl Into<String>) -> Self {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        Self {
            timestamp,
            log_type,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogBuffer {
    capacity: usize,
    entries: VecDeque<LogEntry>,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    pub fn format_all(&self) -> String {
        self.entries
            .iter()
            .map(|e| format!("[{}] [{}] {}", e.timestamp, e.log_type.as_str(), e.message))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

