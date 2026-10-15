mod raw_frame;
pub(crate) mod mumu;
use crate::config::{validate_endpoint, validate_serial};
use serde::Serialize;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct CancelSignal {
    token: CancellationToken,
    dispatch: std::sync::Arc<std::sync::Mutex<()>>,
}
impl CancelSignal {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            dispatch: std::sync::Arc::new(std::sync::Mutex::new(())),
        }
    }
    pub fn cancel(&self) {
        let _guard = self.dispatch.lock().expect("dispatch lock poisoned");
        self.token.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }
    fn spawn(&self, command: &mut Command) -> Result<tokio::process::Child, String> {
        let _guard = self.dispatch.lock().map_err(|e| e.to_string())?;
        if self.is_cancelled() {
            return Err("操作已取消".into());
        }
        command.spawn().map_err(|e| format!("ADB 启动失败：{e}"))
    }
}

#[derive(Clone)]
pub struct Adb {
    pub executable: PathBuf,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Device {
    pub serial: String,
    pub status: String,
    pub description: String,
}

impl Adb {
    async fn verify_dimensions(
        &self,
        serial: &str,
        expected: [u32; 2],
        cancel: &CancelSignal,
    ) -> Result<(), String> {
        let bytes = self.screenshot(serial, cancel).await?;
        let image = crate::vision::decode(&bytes)?;
        ensure_dimensions(expected, [image.width(), image.height()])
    }

    async fn execute(&self, args: &[&str], cancel: &CancelSignal) -> Result<Vec<u8>, String> {
        if cancel.is_cancelled() {
            return Err("操作已取消".into());
        }
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = cancel
            .spawn(&mut command)
            .map_err(|e| format!("{}（{}）", e, self.executable.display()))?;
        let stdout = child.stdout.take().ok_or("ADB stdout 不可用")?;
        let stderr = child.stderr.take().ok_or("ADB stderr 不可用")?;
        let work = async {
            let out = async {
                let mut bytes = Vec::new();
                stdout
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map(|_| bytes)
            };
            let err = async {
                let mut bytes = Vec::new();
                stderr
                    .take(64 * 1024)
                    .read_to_end(&mut bytes)
                    .await
                    .map(|_| bytes)
            };
            let (status, out, err) =
                tokio::try_join!(child.wait(), out, err).map_err(|e| e.to_string())?;
            if out.len() > 64 * 1024 * 1024 {
                return Err("ADB 输出超过 64 MiB 限制".into());
            }
            if !status.success() {
                return Err(format!(
                    "ADB 操作失败：{}",
                    String::from_utf8_lossy(&err).trim()
                ));
            }
            Ok(out)
        };
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("操作已取消".into()),
            result = tokio::time::timeout(Duration::from_secs(12), work) => result.unwrap_or_else(|_| Err("ADB 操作超时（12 秒）".into())),
        };
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    }
    pub async fn devices(&self, cancel: &CancelSignal) -> Result<Vec<Device>, String> {
        let bytes = self.execute(&["devices", "-l"], cancel).await?;
        Ok(parse_devices(&String::from_utf8_lossy(&bytes)))
    }
    pub async fn connect(&self, endpoint: &str, cancel: &CancelSignal) -> Result<String, String> {
        validate_endpoint(endpoint)?;
        let bytes = self.execute(&["connect", endpoint], cancel).await?;
        let response = String::from_utf8_lossy(&bytes).trim().to_owned();
        if !response.starts_with("connected to ") && !response.starts_with("already connected to ")
        {
            return Err(response);
        }
        Ok(response)
    }
    pub async fn screenshot(&self, serial: &str, cancel: &CancelSignal) -> Result<Vec<u8>, String> {
        validate_serial(serial)?;
        self.execute(&["-s", serial, "exec-out", "screencap", "-p"], cancel)
            .await
    }
    /// Prefer uncompressed screencap on local emulators; retain PNG fallback for unsupported devices.
    pub async fn screenshot_fast(&self,serial:&str,cancel:&CancelSignal)->Result<(image::RgbImage,Vec<u8>,String),String>{
        validate_serial(serial)?;
        let raw=self.execute(&["-s",serial,"exec-out","screencap"],cancel).await;
        if cancel.is_cancelled(){return Err("操作已取消".into());}
        if let Ok(bytes)=raw {
            let parsed=tokio::task::spawn_blocking(move||{
                let rgb=raw_frame::parse(&bytes)?;
                let png=raw_frame::png(&rgb)?;
                Ok::<_,String>((rgb,png,"raw".to_string()))
            }).await.map_err(|e|e.to_string())?;
            if let Ok(frame)=parsed{return Ok(frame);}
        }
        let bytes=self.screenshot(serial,cancel).await?;
        tokio::task::spawn_blocking(move||{
            let rgb=crate::vision::detector::decode_rgb(&bytes)?;
            Ok((rgb,bytes,"png-fallback".to_string()))
        }).await.map_err(|e|e.to_string())?
    }
    pub async fn tap(
        &self,
        serial: &str,
        x: u32,
        y: u32,
        dimensions: [u32; 2],
        cancel: &CancelSignal,
    ) -> Result<(), String> {
        validate_serial(serial)?;
        validate_point(x, y, dimensions)?;
        self.verify_dimensions(serial, dimensions, cancel).await?;
        self.execute(
            &[
                "-s",
                serial,
                "shell",
                "input",
                "tap",
                &x.to_string(),
                &y.to_string(),
            ],
            cancel,
        )
        .await
        .map(|_| ())
    }
    /// Dispatch from the exact freshly inferred frame, with no hidden second
    /// screenshot replacing the evidence. The engine owns the device pipeline.
    pub async fn tap_observed(
        &self, serial: &str, point: [u32; 2], dimensions: [u32; 2],
        captured_at: std::time::Instant, cancel: &CancelSignal,
    ) -> Result<(), String> {
        validate_serial(serial)?;
        validate_point(point[0], point[1], dimensions)?;
        if captured_at.elapsed() > Duration::from_millis(1500) {
            return Err("截图已过期，禁止点击；请重新观察".into());
        }
        self.execute(&["-s", serial, "shell", "input", "tap", &point[0].to_string(), &point[1].to_string()], cancel)
            .await.map(|_| ())
    }
    pub async fn swipe(
        &self,
        serial: &str,
        points: [u32; 4],
        duration_ms: u32,
        dimensions: [u32; 2],
        cancel: &CancelSignal,
    ) -> Result<(), String> {
        validate_serial(serial)?;
        validate_point(points[0], points[1], dimensions)?;
        validate_point(points[2], points[3], dimensions)?;
        if !(100..=3000).contains(&duration_ms) {
            return Err("滑动时长必须为 100–3000 ms".into());
        }
        self.verify_dimensions(serial, dimensions, cancel).await?;
        let values = points.map(|x| x.to_string());
        self.execute(
            &[
                "-s",
                serial,
                "shell",
                "input",
                "swipe",
                &values[0],
                &values[1],
                &values[2],
                &values[3],
                &duration_ms.to_string(),
            ],
            cancel,
        )
        .await
        .map(|_| ())
    }
}
pub fn validate_point(x: u32, y: u32, dimensions: [u32; 2]) -> Result<(), String> {
    if x >= dimensions[0] || y >= dimensions[1] {
        return Err(format!(
            "操作位置超出 {}×{} 屏幕边界",
            dimensions[0], dimensions[1]
        ));
    }
    Ok(())
}
pub fn ensure_dimensions(expected: [u32; 2], actual: [u32; 2]) -> Result<(), String> {
    if expected != actual {
        return Err("截图后设备分辨率或方向发生变化，请重新识别再操作".into());
    }
    Ok(())
}
pub fn parse_devices(value: &str) -> Vec<Device> {
    value
        .lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() < 2
                || line.starts_with("List ")
                || line.starts_with('*')
                || validate_serial(parts[0]).is_err()
            {
                return None;
            }
            Some(Device {
                serial: parts[0].into(),
                status: parts[1].into(),
                description: parts[2..].join(" "),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_online_offline_unauthorized() {
        let items = parse_devices(
            "List of devices attached\r\nemulator-5554 offline transport_id:1\r\n127.0.0.1:16384 device model:MuMu\nabc unauthorized\n",
        );
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].status, "offline");
        assert_eq!(items[1].serial, "127.0.0.1:16384");
    }
    #[test]
    fn coordinates_do_not_wrap() {
        assert!(validate_point(1919, 1079, [1920, 1080]).is_ok());
        assert!(validate_point(1920, 0, [1920, 1080]).is_err());
        assert!(validate_point(0, u32::MAX, [1920, 1080]).is_err());
        assert!(validate_point(0, 0, [0, 1080]).is_err());
        assert!(ensure_dimensions([1920, 1080], [1080, 1920]).is_err());
        assert!(ensure_dimensions([1920, 1080], [2560, 1440]).is_err());
        assert!(ensure_dimensions([1920, 1080], [1920, 1080]).is_ok());
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn cancellation_interrupts_a_running_child() {
        let adb = Adb {
            executable: PathBuf::from("powershell.exe"),
        };
        let cancel = CancelSignal::new();
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            signal.cancel();
        });
        let started = std::time::Instant::now();
        let result = adb
            .execute(
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 30",
                ],
                &cancel,
            )
            .await;
        assert!(result.unwrap_err().contains("取消"));
        assert!(started.elapsed() < Duration::from_secs(4));
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn nonzero_exit_is_not_reported_as_success() {
        let adb = Adb {
            executable: PathBuf::from("powershell.exe"),
        };
        assert!(
            adb.execute(
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[Console]::Error.WriteLine('offline'); exit 1"
                ],
                &CancelSignal::new()
            )
            .await
            .unwrap_err()
            .contains("offline")
        );
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn cancelled_signal_never_dispatches_another_child() {
        let adb = Adb {
            executable: PathBuf::from("powershell.exe"),
        };
        let cancel = CancelSignal::new();
        cancel.cancel();
        assert!(
            adb.execute(&["-NoProfile", "-Command", "exit 0"], &cancel)
                .await
                .unwrap_err()
                .contains("取消")
        );
    }
}
