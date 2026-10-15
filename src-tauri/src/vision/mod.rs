pub mod scene;
mod resize;
pub(crate) mod runtime;
mod opencv_matcher;
pub mod detector;
pub mod viewport;
use crate::device::CancelSignal;
use image::{GrayImage, ImageReader};
pub use opencv_matcher::find;
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::Path, time::Instant};

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSpec {
    pub id: String,
    pub file: String,
    pub roi: [u32; 4],
    pub threshold: f32,
}
#[derive(Debug, Clone, Serialize)]
pub struct Match {
    pub id: String,
    pub score: f32,
    pub found: bool,
    pub bounds: [u32; 4],
}
#[derive(Serialize)]
pub struct Diagnosis {
    pub matches: Vec<Match>,
    pub viewport: viewport::Viewport,
    pub alignment_confident: bool,
    pub elapsed_ms: u128,
    pub ocr: String,
    pub matcher: String,
}

pub fn decode(bytes: &[u8]) -> Result<GrayImage, String> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map(|im| im.to_luma8())
        .map_err(|e| format!("PNG 截图解码失败：{e}"))
}

pub struct Templates {
    items: Vec<(TemplateSpec, GrayImage)>,
}
impl Templates {
    pub fn load(resources: &Path) -> Result<Self, String> {
        let root = resources.join("templates");
        let specs: Vec<TemplateSpec> = serde_json::from_slice(
            &std::fs::read(root.join("manifest.json")).map_err(|e| format!("模板清单缺失：{e}"))?,
        )
        .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        for spec in specs {
            if !spec.file.ends_with(".png")
                || spec.file.contains(['/', '\\'])
                || !(0.0..=1.0).contains(&spec.threshold)
            {
                return Err("模板清单不合法".into());
            }
            let image = decode(
                &std::fs::read(root.join(&spec.file))
                    .map_err(|e| format!("模板 {} 缺失：{e}", spec.id))?,
            )?;
            items.push((spec, image));
        }
        Ok(Self { items })
    }
    pub fn diagnose(&self, bytes: &[u8], cancel: &CancelSignal) -> Result<Diagnosis, String> {
        let start = Instant::now();
        let screen = decode(bytes)?;
        if screen.width() == 0 || screen.height() == 0 {
            return Err("截图尺寸为空".into());
        }
        let source_size = [screen.width(), screen.height()];
        let rotations: &[u32] = if screen.height() > screen.width() {
            &[0, 90, 180, 270]
        } else {
            &[0, 180]
        };
        let mut results = Vec::new();
        for &rotation in rotations {
            let oriented = viewport::oriented(&screen, rotation);
            for view in viewport::candidates(&oriented, rotation, source_size) {
                if cancel.is_cancelled() {
                    return Err("识别已取消".into());
                }
                let normalized = view.normalize(&oriented);
                let mut matches = Vec::new();
                for (spec, template) in &self.items {
                    if start.elapsed().as_secs() >= 8 {
                        return Err("自适应识别超过 8 秒预算".into());
                    }
                    let mut adapted = spec.clone();
                    adapted.roi = view.roi(spec.roi);
                    matches.push(find(&normalized, template, &adapted, cancel)?);
                }
                let count = matches.iter().filter(|m| m.found).count();
                let score: f32 = matches.iter().filter(|m| m.found).map(|m| m.score).sum();
                results.push((count, score, view, matches));
            }
        }
        if start.elapsed().as_secs() >= 8 {
            return Err("自适应识别超过 8 秒预算".into());
        }
        results.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.total_cmp(&a.1)));
        let (count, score, view, mut matches) = results.remove(0);
        // Competing alignments with similar evidence but different click positions are unsafe.
        let ambiguous =
            results
                .iter()
                .any(|(other_count, other_score, other_view, other_matches)| {
                    *other_count == count
                        && count > 0
                        && (score - other_score).abs() < 0.02
                        && matches.iter().filter(|m| m.found).any(|m| {
                            other_matches
                                .iter()
                                .filter(|n| n.found && n.id == m.id)
                                .any(|n| {
                                    let a = view
                                        .point(
                                            m.bounds[0] + m.bounds[2] / 2,
                                            m.bounds[1] + m.bounds[3] / 2,
                                        )
                                        .unwrap();
                                    let b = other_view
                                        .point(
                                            n.bounds[0] + n.bounds[2] / 2,
                                            n.bounds[1] + n.bounds[3] / 2,
                                        )
                                        .unwrap();
                                    a[0].abs_diff(b[0]) + a[1].abs_diff(b[1])
                                        > source_size[0].min(source_size[1]) / 50
                                })
                        })
                });
        if ambiguous {
            for found in &mut matches {
                found.found = false;
            }
        }
        Ok(Diagnosis {
            matches,
            viewport: view,
            alignment_confident: count > 0 && !ambiguous,
            elapsed_ms: start.elapsed().as_millis(),
            ocr: "未验证：尚未提供经过许可确认的 OCR 模型与 ONNX Runtime".into(),
            matcher: if ambiguous {
                "多个画面映射冲突，已禁止自动点击".into()
            } else {
                "自适应缩放 / 边缘锚定 / 黑边检测 / 方向识别 + OpenCV 4.12 灰度 TM_CCOEFF_NORMED"
                    .into()
            },
        })
    }
}

// Search only the upstream ROI. Sample pixels for bounded CPU cost, then refine at full resolution.
#[cfg(test)]
fn rust_find(
    screen: &GrayImage,
    template: &GrayImage,
    spec: &TemplateSpec,
    cancel: &CancelSignal,
) -> Result<Match, String> {
    let [rx, ry, rw, rh] = spec.roi;
    let (tw, th) = template.dimensions();
    if tw == 0
        || th == 0
        || tw > rw
        || th > rh
        || rx.checked_add(rw).is_none_or(|v| v > screen.width())
        || ry.checked_add(rh).is_none_or(|v| v > screen.height())
    {
        return Err(format!("模板 {} 的搜索区域无效", spec.id));
    }
    let score = |x: u32, y: u32, step: usize| {
        let (mut a, mut b, mut aa, mut bb, mut ab, mut n) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
        for ty in (0..th).step_by(step) {
            for tx in (0..tw).step_by(step) {
                let av = f64::from(screen.get_pixel(x + tx, y + ty)[0]);
                let bv = f64::from(template.get_pixel(tx, ty)[0]);
                a += av;
                b += bv;
                aa += av * av;
                bb += bv * bv;
                ab += av * bv;
                n += 1.0;
            }
        }
        let divisor = ((aa - a * a / n).max(0.0) * (bb - b * b / n).max(0.0)).sqrt();
        if divisor < 1e-8 {
            return -1.0;
        }
        ((ab - a * b / n) / divisor) as f32
    };
    let mut best = (-1.0f32, rx, ry);
    for y in (ry..=ry + rh - th).step_by(2) {
        if cancel.is_cancelled() {
            return Err("识别已取消".into());
        }
        for x in (rx..=rx + rw - tw).step_by(2) {
            let value = score(x, y, 3);
            if value > best.0 {
                best = (value, x, y);
            }
        }
    }
    let (_, bx, by) = best;
    best.0 = -1.0;
    for y in by.saturating_sub(2).max(ry)..=(by + 2).min(ry + rh - th) {
        for x in bx.saturating_sub(2).max(rx)..=(bx + 2).min(rx + rw - tw) {
            let value = score(x, y, 1);
            if value > best.0 {
                best = (value, x, y);
            }
        }
    }
    Ok(Match {
        id: spec.id.clone(),
        score: best.0,
        found: best.0 >= spec.threshold,
        bounds: [best.1, best.2, tw, th],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locates_shifted_template_and_rejects_flat_screen() {
        let mut screen = GrayImage::new(30, 24);
        let template =
            GrayImage::from_fn(6, 6, |x, y| image::Luma([((x * 73 + y * 39) % 255) as u8]));
        image::imageops::replace(&mut screen, &template, 11, 9);
        let spec = TemplateSpec {
            id: "test".into(),
            file: "test.png".into(),
            roi: [0, 0, 30, 24],
            threshold: 0.9,
        };
        let result = find(&screen, &template, &spec, &CancelSignal::new()).unwrap();
        assert!(result.found);
        assert_eq!(result.bounds, [11, 9, 6, 6]);
        assert!(
            !find(
                &GrayImage::new(30, 24),
                &template,
                &spec,
                &CancelSignal::new()
            )
            .unwrap()
            .found
        );
    }
    #[test]
    fn corrupt_png_is_rejected() {
        assert!(decode(b"not png").is_err());
    }
    #[test]
    fn cancelled_recognition_does_not_return_a_target() {
        let token = CancelSignal::new();
        token.cancel();
        let spec = TemplateSpec {
            id: "x".into(),
            file: "x.png".into(),
            roi: [0, 0, 5, 5],
            threshold: 0.9,
        };
        assert!(find(&GrayImage::new(5, 5), &GrayImage::new(2, 2), &spec, &token).is_err());
    }
    #[test]
    fn upstream_templates_load_and_match_at_their_original_positions() {
        let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let templates = Templates::load(&resources).unwrap();
        assert_eq!(templates.items.len(), 11);
        for (spec, template) in templates.items {
            let mut screen = GrayImage::new(1280, 720);
            let x = spec.roi[0];
            let y = spec.roi[1];
            image::imageops::replace(&mut screen, &template, i64::from(x), i64::from(y));
            let result = find(&screen, &template, &spec, &CancelSignal::new()).unwrap();
            assert!(result.found, "{} not found", spec.id);
            assert_eq!(result.bounds, [x, y, template.width(), template.height()]);
        }
    }

    fn png(screen: GrayImage) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(screen)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    #[test]
    fn adaptive_recognition_maps_real_templates_back_to_device_pixels() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let templates = Templates::load(&root).unwrap();
        let mut reference = GrayImage::from_pixel(1280, 720, image::Luma([40]));
        for (id, x, y) in [
            ("challenge", 1133, 584),
            ("layer_11", 929, 123),
            ("locked", 706, 659),
        ] {
            let (_, template) = templates
                .items
                .iter()
                .find(|(spec, _)| spec.id == id)
                .unwrap();
            image::imageops::replace(&mut reference, template, x, y);
        }
        for [w, h] in [
            [960, 540],
            [1920, 1080],
            [2560, 1440],
            [3840, 2160],
            [1024, 768],
            [2340, 1080],
        ] {
            let screen =
                image::imageops::resize(&reference, w, h, image::imageops::FilterType::Triangle);
            let report = templates
                .diagnose(&png(screen), &CancelSignal::new())
                .unwrap();
            assert!(report.alignment_confident, "{w}x{h}");
            let target = report
                .matches
                .iter()
                .find(|m| m.id == "challenge" && m.found)
                .unwrap();
            let [x, y, tw, th] = target.bounds;
            let actual = report.viewport.point(x + tw / 2, y + th / 2).unwrap();
            let expected = [
                (1188f64 * f64::from(w) / 1280.0) as u32,
                (613f64 * f64::from(h) / 720.0) as u32,
            ];
            assert!(
                actual[0].abs_diff(expected[0]) <= 3 && actual[1].abs_diff(expected[1]) <= 3,
                "{w}x{h}: {actual:?} vs {expected:?}"
            );
        }
        let mut boxed = GrayImage::new(1600, 1000);
        image::imageops::replace(&mut boxed, &reference, 160, 140);
        let report = templates
            .diagnose(&png(boxed), &CancelSignal::new())
            .unwrap();
        assert!(report.alignment_confident);
        assert_eq!(report.viewport.content_rect, [160, 140, 1280, 720]);
        assert_eq!(
            report.viewport.reference_point(1175, 350).unwrap(),
            [1335, 490]
        );
        let mut portrait = GrayImage::new(720, 1280);
        let content =
            image::imageops::resize(&reference, 720, 405, image::imageops::FilterType::Triangle);
        image::imageops::replace(&mut portrait, &content, 0, 437);
        let report = templates
            .diagnose(&png(portrait), &CancelSignal::new())
            .unwrap();
        assert!(report.alignment_confident);
        assert_eq!(report.viewport.rotation, 0);
        assert_eq!(report.viewport.content_rect, [0, 437, 720, 405]);
        for (screen, rotation) in [
            (image::imageops::rotate90(&reference), 270),
            (image::imageops::rotate270(&reference), 90),
            (image::imageops::rotate180(&reference), 180),
        ] {
            let report = templates
                .diagnose(&png(screen), &CancelSignal::new())
                .unwrap();
            assert!(report.alignment_confident);
            assert_eq!(report.viewport.rotation, rotation);
        }
        let mut wide = GrayImage::from_pixel(1560, 720, image::Luma([40]));
        for (id, x, y) in [
            ("challenge", 1413, 584),
            ("layer_11", 1209, 123),
            ("locked", 846, 659),
        ] {
            let (_, template) = templates
                .items
                .iter()
                .find(|(spec, _)| spec.id == id)
                .unwrap();
            image::imageops::replace(&mut wide, template, x, y);
        }
        let report = templates
            .diagnose(&png(wide), &CancelSignal::new())
            .unwrap();
        assert_eq!(report.viewport.strategy, "anchored");
        assert!(report.alignment_confident);
        let target = report
            .matches
            .iter()
            .find(|m| m.id == "challenge" && m.found)
            .unwrap();
        let [x, y, w, h] = target.bounds;
        assert_eq!(
            report.viewport.point(x + w / 2, y + h / 2).unwrap(),
            [1468, 613]
        );
        assert_eq!(
            report.viewport.reference_point(1175, 350).unwrap(),
            [1455, 350]
        );
    }

    #[test]
    fn competing_orientations_disable_all_click_targets() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let templates = Templates::load(&root).unwrap();
        let mut screen = GrayImage::from_pixel(1280, 720, image::Luma([40]));
        for (id, x, y) in [
            ("challenge", 1133, 584),
            ("layer_11", 929, 123),
            ("locked", 706, 659),
        ] {
            let (_, template) = templates
                .items
                .iter()
                .find(|(spec, _)| spec.id == id)
                .unwrap();
            image::imageops::replace(&mut screen, template, x, y);
            image::imageops::replace(
                &mut screen,
                &image::imageops::rotate180(template),
                1280 - x - i64::from(template.width()),
                720 - y - i64::from(template.height()),
            );
        }
        let report = templates
            .diagnose(&png(screen), &CancelSignal::new())
            .unwrap();
        assert!(!report.alignment_confident);
        assert!(report.matches.iter().all(|m| !m.found));
        assert!(report.matcher.contains("冲突"));
    }
}
