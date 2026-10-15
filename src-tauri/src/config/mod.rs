pub mod import;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub endpoint: String,
    pub serial: String,
    pub soul: SoulConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SoulConfig {
    pub rounds: u32,
    pub max_minutes: u32,
    pub stage_timeout_seconds: u32,
    pub layer: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            endpoint: "127.0.0.1:16384".into(),
            serial: String::new(),
            soul: SoulConfig {
                rounds: 1,
                max_minutes: 30,
                stage_timeout_seconds: 180,
                layer: 1,
            },
        }
    }
}

#[derive(Serialize)]
pub struct Limits {
    pub rounds: [u32; 2],
    pub max_minutes: [u32; 2],
    pub stage_timeout_seconds: [u32; 2],
    pub layers: [u32; 5],
}
pub const LIMITS: Limits = Limits {
    rounds: [1, 1000],
    max_minutes: [1, 1440],
    stage_timeout_seconds: [30, 600],
    layers: [1, 10, 11, 12, 13],
};

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("配置版本不受支持；请勿直接使用旧 OAS JSON".into());
        }
        validate_endpoint(&self.endpoint)?;
        if !self.serial.is_empty() {
            validate_serial(&self.serial)?;
        }
        for (name, value, range) in [
            ("轮数", self.soul.rounds, LIMITS.rounds),
            ("运行分钟", self.soul.max_minutes, LIMITS.max_minutes),
            (
                "阶段超时",
                self.soul.stage_timeout_seconds,
                LIMITS.stage_timeout_seconds,
            ),
        ] {
            if !(range[0]..=range[1]).contains(&value) {
                return Err(format!("{name}必须在 {}–{} 之间", range[0], range[1]));
            }
        }
        if !LIMITS.layers.contains(&self.soul.layer) {
            return Err("层数配置无效；视觉闭环当前支持壹层，10–13层仅保留旧配置".into());
        }
        Ok(())
    }
}

pub fn validate_serial(serial: &str) -> Result<(), String> {
    if serial.is_empty()
        || serial.len() > 128
        || serial.starts_with('-')
        || !serial
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:_-".contains(c))
    {
        return Err("无效设备标识".into());
    }
    Ok(())
}

pub fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .map_err(|_| "请输入本机 IPv4 地址和端口，如 127.0.0.1:16384")?;
    if !addr.ip().is_loopback() || !addr.is_ipv4() || addr.port() == 0 {
        return Err("首版仅连接本机模拟器的 IPv4 回环地址".into());
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let data = fs::read(path).map_err(|e| format!("读取配置失败：{e}"))?;
    let config: Config =
        serde_json::from_slice(&data).map_err(|e| format!("配置损坏，原文件已保留：{e}"))?;
    config.validate()?;
    Ok(config)
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| format!("原子保存失败：{e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_ipc_arguments() {
        for value in ["-s", "a b", "a;exit", "../x", ""] {
            assert!(validate_serial(value).is_err());
        }
        for value in ["192.168.1.2:5555", "localhost:5555", "127.0.0.1:0"] {
            assert!(validate_endpoint(value).is_err());
        }
        assert!(validate_serial("emulator-5554").is_ok());
        assert!(validate_endpoint("127.0.0.1:16384").is_ok());
    }
    #[test]
    fn config_bounds_and_version() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());
        config.soul.rounds = 0;
        assert!(config.validate().is_err());
        config.soul.rounds = 1;
        config.schema_version = 2;
        assert!(config.validate().is_err());
    }
    #[test]
    fn save_replaces_and_corruption_is_preserved() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&dir).unwrap();
        let path = dir.join("config.json");
        atomic_json(&path, &Config::default()).unwrap();
        let mut config = Config::default();
        config.soul.rounds = 2;
        atomic_json(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        fs::write(&path, b"bad").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"bad");
        fs::remove_dir_all(dir).unwrap();
    }
}
