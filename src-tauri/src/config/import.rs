use super::{Config, atomic_json, validate_endpoint, validate_serial};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, fs, io::Write, path::Path};

pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Serialize)]
pub struct Mapping {
    pub source: String,
    pub target: String,
    pub before: Value,
    pub after: Value,
}
#[derive(Serialize)]
pub struct Preview {
    pub base: Config,
    pub config: Config,
    pub mappings: Vec<Mapping>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub ignored: Vec<String>,
}
fn leaves(value: &Value, path: &str, output: &mut Vec<String>) -> Result<(), String> {
    if output.len() >= 4096 {
        return Err("配置字段超过4096项，无法导入".into());
    }
    if let Some(object) = value.as_object().filter(|o| !o.is_empty()) {
        for (key, value) in object {
            leaves(
                value,
                &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                output,
            )?;
        }
    } else {
        output.push(path.into());
    }
    Ok(())
}
pub fn preview(content: &str, base: &Config) -> Result<Preview, String> {
    if content.len() > MAX_BYTES {
        return Err("配置文件不能超过1 MiB".into());
    }
    let value: Value = serde_json::from_str(content.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("旧配置不是有效JSON：{e}"))?;
    if !value
        .pointer("/script/device")
        .is_some_and(Value::is_object)
        && !value
            .pointer("/orochi/orochi_config")
            .is_some_and(Value::is_object)
    {
        return Err("未找到 OAS script.device 或 orochi.orochi_config；不支持此配置结构".into());
    }
    base.validate()?;
    let mut result = Preview { base: base.clone(), config: base.clone(), mappings: vec![], warnings: vec![
        "仅迁移设备和御魂运行参数；不会导入任务启用状态、旧队列或启动任务。现有队列保留入队时的配置。".into(),
        "新引擎仅支持单人流程，需手动进入目标层、锁定阵容并开启自动战斗；不会迁移组队、加成或换御魂行为。".into(),
    ], errors: vec![], ignored: vec![] };
    let mut consumed = HashSet::new();
    for (source, target) in [
        ("/script/device/serial", "/serial"),
        ("/orochi/orochi_config/layer", "/soul/layer"),
        ("/orochi/orochi_config/limit_count", "/soul/rounds"),
        ("/orochi/orochi_config/limit_time", "/soul/max_minutes"),
    ] {
        let Some(old) = value.pointer(source) else {
            continue;
        };
        let converted: Result<Value, String> = match target {
            "/serial" => match old.as_str() {
                Some("auto" | "") => {
                    result.warnings.push(
                        "旧设备号为auto或空值，保留当前设备；请在设备页选择具体设备。".into(),
                    );
                    continue;
                }
                Some(serial) => validate_serial(serial).map(|_| Value::from(serial)),
                None => Err("设备号必须是字符串".into()),
            },
            "/soul/layer" => match old.as_str() {
                Some("拾层") => Ok(Value::from(10)),
                Some("悲鸣") => Ok(Value::from(11)),
                Some("神罚") => Ok(Value::from(12)),
                Some("虚无") => Ok(Value::from(13)),
                _ => Err("仅支持拾层、悲鸣、神罚、虚无".into()),
            },
            "/soul/rounds" => old
                .as_u64()
                .filter(|n| (1..=1000).contains(n))
                .map(Value::from)
                .ok_or_else(|| "次数须为1–1000整数，不迁移无限次数".into()),
            _ => duration_minutes(old).map(Value::from),
        };
        match converted {
            Ok(after) => {
                let mut config = serde_json::to_value(&result.config).map_err(|e| e.to_string())?;
                let before = config.pointer(target).expect("known config field").clone();
                *config.pointer_mut(target).expect("known config field") = after.clone();
                result.config = serde_json::from_value(config).map_err(|e| e.to_string())?;
                result.mappings.push(Mapping {
                    source: source.into(),
                    target: target.into(),
                    before,
                    after,
                });
                consumed.insert(source.to_string());
                if target == "/serial" && validate_endpoint(result.config.serial.as_str()).is_ok() {
                    result.mappings.push(Mapping {
                        source: source.into(),
                        target: "/endpoint".into(),
                        before: Value::from(result.config.endpoint.clone()),
                        after: Value::from(result.config.serial.clone()),
                    });
                    result.config.endpoint = result.config.serial.clone();
                }
            }
            Err(error) => result.errors.push(format!("{source}：{error}")),
        }
    }
    let mut paths = vec![];
    leaves(&value, "", &mut paths)?;
    result.ignored = paths
        .into_iter()
        .filter(|p| !consumed.contains(p))
        .collect();
    if result.mappings.is_empty() {
        result.errors.push("没有可导入的兼容字段".into());
    }
    result.config.validate()?;
    Ok(result)
}
fn duration_minutes(value: &Value) -> Result<u32, String> {
    let invalid =
        || "运行时长须为HH:MM:SS，整分钟且在1–1440分钟内；不迁移无限时长或秒级预算".to_string();
    let text = value.as_str().ok_or_else(invalid)?;
    let parts: Vec<_> = text.split(':').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.len() != 2 || !p.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let hours: u32 = parts[0].parse().map_err(|_| invalid())?;
    let minutes: u32 = parts[1].parse().map_err(|_| invalid())?;
    if minutes > 59 || parts[2] != "00" {
        return Err(invalid());
    }
    let total = hours * 60 + minutes;
    if !(1..=1440).contains(&total) {
        return Err(invalid());
    }
    Ok(total)
}
pub fn apply(
    directory: &Path,
    content: &str,
    current: &Config,
    expected_base: &Config,
) -> Result<(Preview, String), String> {
    if current != expected_base {
        return Err("当前配置已变化，请重新预览后导入".into());
    }
    let report = preview(content, current)?;
    if !report.errors.is_empty() {
        return Err("导入含不兼容字段错误，请先修正文件并重新预览".into());
    }
    let backup = directory
        .join("imports")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&backup).map_err(|e| format!("无法创建导入备份：{e}"))?;
    let mut source = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(backup.join("oas-original.json"))
        .map_err(|e| e.to_string())?;
    source
        .write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    source.sync_all().map_err(|e| e.to_string())?;
    atomic_json(&backup.join("config-before.json"), current)?;
    atomic_json(&backup.join("report.json"), &report)?;
    // All backups must succeed before replacing the active configuration.
    atomic_json(&directory.join("config.json"), &report.config)?;
    Ok((report, backup.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"{"script":{"device":{"serial":"127.0.0.1:16384"}},"orochi":{"scheduler":{"enable":true},"orochi_config":{"layer":"神罚","limit_count":40,"limit_time":"01:30:00","user_status":"leader"}}}"#;
    #[test]
    fn upstream_template_subset_is_compatible_without_activating_old_scheduler() {
        let report = preview(
            include_str!("../../tests/fixtures/oas-template-subset.json"),
            &Config::default(),
        )
        .unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.config.soul.rounds, 30);
        assert_eq!(report.config.soul.layer, 11);
        assert!(report.config.serial.is_empty());
        assert!(
            report
                .ignored
                .contains(&"/script/device/control_method".into())
        );
        assert!(report.ignored.contains(&"/orochi/scheduler/enable".into()));
        assert!(
            report
                .ignored
                .contains(&"/orochi/switch_soul/enable".into())
        );
        for (label, layer) in [("拾层", 10), ("悲鸣", 11), ("神罚", 12), ("虚无", 13)] {
            assert_eq!(
                preview(&SOURCE.replace("神罚", label), &Config::default())
                    .unwrap()
                    .config
                    .soul
                    .layer,
                layer
            );
        }
    }
    #[test]
    fn maps_supported_fields_and_reports_unmigrated_behavior() {
        let report = preview(SOURCE, &Config::default()).unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.config.soul.layer, 12);
        assert_eq!(report.config.soul.rounds, 40);
        assert_eq!(report.config.soul.max_minutes, 90);
        assert_eq!(report.config.serial, "127.0.0.1:16384");
        assert_eq!(report.config.soul.stage_timeout_seconds, 180);
        assert!(report.ignored.contains(&"/orochi/scheduler/enable".into()));
        assert!(
            report
                .ignored
                .contains(&"/orochi/orochi_config/user_status".into())
        );
    }
    #[test]
    fn rejects_invalid_limits_structures_and_large_files() {
        for time in ["00:00:00", "00:30:01", "00:60:00", "25:00:00", "1:00:00"] {
            assert!(duration_minutes(&Value::from(time)).is_err());
        }
        assert!(preview("{}", &Config::default()).is_err());
        assert!(preview(&" ".repeat(MAX_BYTES + 1), &Config::default()).is_err());
        assert!(
            !preview(&SOURCE.replace("40", "0"), &Config::default())
                .unwrap()
                .errors
                .is_empty()
        );
        assert!(
            !preview(&SOURCE.replace("神罚", "壹层"), &Config::default())
                .unwrap()
                .errors
                .is_empty()
        );
    }
    #[test]
    fn auto_serial_preserves_current_selection() {
        let base = Config {
            serial: "emulator-5554".into(),
            ..Config::default()
        };
        let report = preview(&SOURCE.replace("127.0.0.1:16384", "auto"), &base).unwrap();
        assert_eq!(report.config.serial, base.serial);
        assert!(report.ignored.contains(&"/script/device/serial".into()));
    }
    #[test]
    fn import_backs_up_exact_source_and_rejects_stale_preview() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        let base = Config::default();
        atomic_json(&dir.join("config.json"), &base).unwrap();
        let source = format!("\u{feff}{SOURCE}\n");
        let (report, backup) = apply(&dir, &source, &base, &base).unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&backup).join("oas-original.json")).unwrap(),
            source
        );
        assert_eq!(
            super::super::load(&Path::new(&backup).join("config-before.json")).unwrap(),
            base
        );
        assert_eq!(
            super::super::load(&dir.join("config.json")).unwrap(),
            report.config
        );
        assert!(apply(&dir, SOURCE, &report.config, &base).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backup_failure_leaves_active_config_unchanged() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        let base = Config::default();
        atomic_json(&dir.join("config.json"), &base).unwrap();
        fs::write(dir.join("imports"), b"blocked").unwrap();
        assert!(apply(&dir, SOURCE, &base, &base).is_err());
        assert_eq!(super::super::load(&dir.join("config.json")).unwrap(), base);
        fs::remove_dir_all(dir).unwrap();
    }
}
