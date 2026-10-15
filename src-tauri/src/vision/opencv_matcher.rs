use super::{Match, TemplateSpec};
use crate::device::CancelSignal;
use image::GrayImage;
use opencv::{core, imgproc, prelude::*};

pub fn find(
    screen: &GrayImage,
    template: &GrayImage,
    spec: &TemplateSpec,
    cancel: &CancelSignal,
) -> Result<Match, String> {
    if cancel.is_cancelled() {
        return Err("识别已取消".into());
    }
    let [x, y, width, height] = spec.roi;
    let (tw, th) = template.dimensions();
    if tw == 0
        || th == 0
        || tw > width
        || th > height
        || x.checked_add(width)
            .is_none_or(|right| right > screen.width())
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > screen.height())
    {
        return Err(format!("模板 {} 的搜索区域无效", spec.id));
    }
    let mut found = Match {
        id: spec.id.clone(),
        score: -1.0,
        found: false,
        bounds: [x, y, tw, th],
    };
    // OpenCV reports 1 for constant templates under CCOEFF_NORMED. They carry no location evidence.
    if template
        .as_raw()
        .iter()
        .all(|pixel| *pixel == template.as_raw()[0])
    {
        return Ok(found);
    }
    let result = (|| -> opencv::Result<()> {
        let crop = image::imageops::crop_imm(screen, x, y, width, height).to_image();
        let pixels = Mat::from_slice(crop.as_raw())?;
        let input = pixels.reshape(1, height as i32)?;
        let template_pixels = Mat::from_slice(template.as_raw())?;
        let target = template_pixels.reshape(1, th as i32)?;
        let mut scores = Mat::default();
        imgproc::match_template_def(&input, &target, &mut scores, imgproc::TM_CCOEFF_NORMED)?;
        let mut score = 0.0;
        let mut point = core::Point::default();
        core::min_max_loc(
            &scores,
            None,
            Some(&mut score),
            None,
            Some(&mut point),
            &core::no_array(),
        )?;
        if score.is_finite() {
            found.score = (score as f32).clamp(-1.0, 1.0);
            found.found = found.score >= spec.threshold;
            found.bounds = [x + point.x as u32, y + point.y as u32, tw, th];
        }
        Ok(())
    })();
    result.map_err(|e| format!("OpenCV匹配 {} 失败：{e}", spec.id))?;
    if cancel.is_cancelled() {
        return Err("识别已取消".into());
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_matches_reference_and_rejects_constant_template() {
        let template = GrayImage::from_fn(13, 11, |x, y| {
            image::Luma([((x * 37 + y * 71 + x * y * 11) % 251) as u8])
        });
        let mut screen = GrayImage::new(50, 40);
        image::imageops::replace(&mut screen, &template, 17, 13);
        let spec = TemplateSpec {
            id: "comparison".into(),
            file: String::new(),
            roi: [3, 5, 44, 32],
            threshold: 0.9,
        };
        let token = CancelSignal::new();
        let native = find(&screen, &template, &spec, &token).unwrap();
        let reference = super::super::rust_find(&screen, &template, &spec, &token).unwrap();
        assert!(native.found && reference.found);
        assert_eq!(native.bounds, reference.bounds);
        assert!((native.score - reference.score).abs() < 0.0001);
        let flat = GrayImage::new(13, 11);
        assert!(!find(&screen, &flat, &spec, &token).unwrap().found);
        assert!(
            !find(&GrayImage::new(50, 40), &template, &spec, &token)
                .unwrap()
                .found
        );
        token.cancel();
        assert!(find(&screen, &template, &spec, &token).is_err());
    }
}
