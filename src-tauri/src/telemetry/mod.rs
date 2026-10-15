use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub level: String,
    pub device_id: String,
    pub run_id: Option<String>,
    pub message: String,
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn append(path: &Path, log: &Log) -> Result<(), String> {
    if fs::metadata(path).is_ok_and(|m| m.len() > 5 * 1024 * 1024) {
        let old = path.with_extension("previous.jsonl");
        if old.exists() {
            fs::remove_file(&old).map_err(|e| e.to_string())?;
        }
        fs::rename(path, old).map_err(|e| e.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, log).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())
}
