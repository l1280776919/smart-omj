use image::{GrayImage, imageops};
use serde::Serialize;

pub const BASE: [u32; 2] = [1280, 720];

/// All rectangles use half-open pixel bounds in the clockwise-rotated image.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Viewport {
    pub source_size: [u32; 2],
    pub rotation: u32,
    pub content_rect: [u32; 4],
    pub working_size: [u32; 2],
    pub strategy: String,
}

impl Viewport {
    pub fn point(&self, x: u32, y: u32) -> Result<[u32; 2], String> {
        if x >= self.working_size[0] || y >= self.working_size[1] {
            return Err("识别坐标超出归一化画面".into());
        }
        let [left, top, width, height] = self.content_rect;
        // Pixel-centre mapping; floor guarantees the last pixel stays inside the viewport.
        let ox = left
            + (((f64::from(x) + 0.5) * f64::from(width) / f64::from(self.working_size[0])).floor()
                as u32)
                .min(width - 1);
        let oy = top
            + (((f64::from(y) + 0.5) * f64::from(height) / f64::from(self.working_size[1])).floor()
                as u32)
                .min(height - 1);
        let [sw, sh] = self.source_size;
        let result = match self.rotation {
            0 => [ox, oy],
            90 => [oy, sh - 1 - ox],
            180 => [sw - 1 - ox, sh - 1 - oy],
            270 => [sw - 1 - oy, ox],
            _ => return Err("未知截图旋转方向".into()),
        };
        if result[0] >= sw || result[1] >= sh {
            return Err("映射坐标超出设备屏幕".into());
        }
        Ok(result)
    }

    pub fn reference_point(&self, x: u32, y: u32) -> Result<[u32; 2], String> {
        if x >= BASE[0] || y >= BASE[1] {
            return Err("任务基准坐标越界".into());
        }
        let [dx, dy] = self.offset(x, y);
        self.point(x + dx, y + dy)
    }

    fn offset(&self, x: u32, y: u32) -> [u32; 2] {
        if self.strategy != "anchored" {
            return [0, 0];
        }
        fn shift(position: u32, base: u32, working: u32) -> u32 {
            let extra = working.saturating_sub(base);
            if position < base / 3 {
                0
            } else if position >= base * 2 / 3 {
                extra
            } else {
                extra / 2
            }
        }
        [
            shift(x, BASE[0], self.working_size[0]),
            shift(y, BASE[1], self.working_size[1]),
        ]
    }

    pub fn roi(&self, roi: [u32; 4]) -> [u32; 4] {
        let [x, y, w, h] = roi;
        let [dx, dy] = self.offset(x + w / 2, y + h / 2);
        [x + dx, y + dy, w, h]
    }

    pub fn normalize(&self, oriented: &GrayImage) -> GrayImage {
        let [x, y, w, h] = self.content_rect;
        let cropped = imageops::crop_imm(oriented, x, y, w, h).to_image();
        imageops::resize(
            &cropped,
            self.working_size[0],
            self.working_size[1],
            imageops::FilterType::Triangle,
        )
    }
}

pub fn oriented(screen: &GrayImage, rotation: u32) -> GrayImage {
    match rotation {
        90 => imageops::rotate90(screen),
        180 => imageops::rotate180(screen),
        270 => imageops::rotate270(screen),
        _ => screen.clone(),
    }
}

pub fn candidates(screen: &GrayImage, rotation: u32, source_size: [u32; 2]) -> Vec<Viewport> {
    let (w, h) = screen.dimensions();
    let base = Viewport {
        source_size,
        rotation,
        content_rect: [0, 0, w, h],
        working_size: BASE,
        strategy: "stretch".into(),
    };
    let mut result = vec![base.clone()];
    // Preserve UI scale and anchor left/centre/right (top/middle/bottom) for expanded layouts.
    let scale = (f64::from(w) / f64::from(BASE[0])).min(f64::from(h) / f64::from(BASE[1]));
    let working_size = [
        (f64::from(w) / scale).round() as u32,
        (f64::from(h) / scale).round() as u32,
    ];
    if working_size != BASE && working_size[0] <= 4096 && working_size[1] <= 4096 {
        result.push(Viewport {
            working_size,
            strategy: "anchored".into(),
            ..base.clone()
        });
    }
    // Black bars are only a hypothesis; recognition evidence selects the final transform.
    if let Some(rect) = black_bar_rect(screen) {
        result.push(Viewport {
            content_rect: rect,
            strategy: "letterbox".into(),
            ..base
        });
    }
    result
}

fn black_bar_rect(screen: &GrayImage) -> Option<[u32; 4]> {
    let (w, h) = screen.dimensions();
    let row_black = |y| (0..w).step_by(4).all(|x| screen.get_pixel(x, y)[0] <= 12);
    let col_black = |x| (0..h).step_by(4).all(|y| screen.get_pixel(x, y)[0] <= 12);
    let top = (0..h / 2).take_while(|&y| row_black(y)).count() as u32;
    let bottom = (0..h / 2).take_while(|&y| row_black(h - 1 - y)).count() as u32;
    let left = (0..w / 2).take_while(|&x| col_black(x)).count() as u32;
    let right = (0..w / 2).take_while(|&x| col_black(w - 1 - x)).count() as u32;
    let cw = w - left - right;
    let ch = h - top - bottom;
    if (left + right + top + bottom == 0) || cw < 32 || ch < 32 {
        return None;
    }
    Some([left, top, cw, ch])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_arbitrary_sizes_and_edges() {
        for [w, h] in [
            [640, 360],
            [1920, 1080],
            [2560, 1440],
            [3840, 2160],
            [1024, 768],
            [2340, 1080],
            [901, 507],
        ] {
            let v =
                candidates(&GrayImage::from_pixel(w, h, image::Luma([30])), 0, [w, h]).remove(0);
            let p = v.reference_point(640, 360).unwrap();
            assert!((i64::from(p[0]) - i64::from(w / 2)).abs() <= 2);
            for (x, y) in [(0, 0), (1279, 719)] {
                let p = v.point(x, y).unwrap();
                assert!(p[0] < w && p[1] < h);
            }
            assert!(v.point(1280, 0).is_err());
        }
    }
    #[test]
    fn rotations_round_trip_known_pixels() {
        let source = GrayImage::from_fn(720, 1280, |x, y| image::Luma([((x + y) % 255) as u8]));
        for rotation in [90, 270] {
            let rotated = oriented(&source, rotation);
            let v = candidates(&rotated, rotation, [720, 1280]).remove(0);
            for (x, y) in [(0, 0), (1279, 719), (1111, 317)] {
                let p = v.point(x, y).unwrap();
                assert_eq!(source.get_pixel(p[0], p[1]), rotated.get_pixel(x, y));
            }
        }
    }
    #[test]
    fn black_bars_and_anchored_layout_preserve_offsets() {
        let mut screen = GrayImage::new(1920, 1200);
        imageops::replace(
            &mut screen,
            &GrayImage::from_pixel(1920, 1080, image::Luma([50])),
            0,
            60,
        );
        let all = candidates(&screen, 0, [1920, 1200]);
        let bars = all.iter().find(|v| v.strategy == "letterbox").unwrap();
        assert_eq!(bars.content_rect, [0, 60, 1920, 1080]);
        assert_eq!(bars.point(0, 0).unwrap(), [0, 60]);
        let wide = candidates(
            &GrayImage::from_pixel(2340, 1080, image::Luma([40])),
            0,
            [2340, 1080],
        );
        let anchor = wide.iter().find(|v| v.strategy == "anchored").unwrap();
        assert_eq!(anchor.working_size, [1560, 720]);
        assert_eq!(anchor.roi([1122, 572, 131, 124]), [1402, 572, 131, 124]);
        assert_eq!(anchor.reference_point(1175, 350).unwrap(), [2183, 525]);
    }
}
