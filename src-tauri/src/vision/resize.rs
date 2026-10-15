use image::RgbImage;
use opencv::{core, imgproc, prelude::*};

/// OpenCV's native bilinear resize, matching YOLO's inference letterbox convention.
/// Keep RGB byte order; OpenCV does not perform a color conversion here.
pub(super) fn linear(rgb:&RgbImage,width:u32,height:u32)->Result<RgbImage,String>{
    if rgb.width()==0||rgb.height()==0||width==0||height==0{return Err("缩放尺寸为空".into());}
    let result=(||->opencv::Result<RgbImage>{
        let pixels=Mat::from_slice(rgb.as_raw())?;
        let input=pixels.reshape(3,rgb.height() as i32)?;
        let mut output=Mat::default();
        imgproc::resize(&input,&mut output,core::Size::new(width as i32,height as i32),0.,0.,imgproc::INTER_LINEAR)?;
        Ok(RgbImage::from_raw(width,height,output.data_bytes()?.to_vec()).expect("OpenCV contiguous RGB output"))
    })();
    result.map_err(|e|format!("图像缩放失败：{e}"))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn resize_keeps_rgb_and_dimensions(){let rgb=RgbImage::from_pixel(12,8,image::Rgb([255,17,3]));let out=linear(&rgb,6,4).unwrap();assert_eq!(out.dimensions(),(6,4));assert!(out.pixels().all(|p|p.0==[255,17,3]));assert!(linear(&rgb,0,4).is_err());}
}
