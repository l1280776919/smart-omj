//! Validated Android screencap RGBA8888 transport (legacy 12-byte or modern 16-byte header).
use image::ImageEncoder;
pub(super) fn parse(bytes:&[u8])->Result<image::RgbImage,String>{
 if bytes.len()<12{return Err("原始截图头不完整".into());}
 let word=|i|u32::from_le_bytes(bytes[i..i+4].try_into().unwrap());
 let (w,h,format)=(word(0),word(4),word(8));
 if w==0||h==0||w>8192||h>8192||format!=1{return Err("原始截图尺寸或像素格式不支持".into());}
 let count=(w as usize).checked_mul(h as usize).and_then(|n|n.checked_mul(4)).ok_or("原始截图尺寸溢出")?;
 if count>64*1024*1024{return Err("原始截图超过大小限制".into());}
 let offset=if bytes.len()==count+12{12}else if bytes.len()==count+16{16}else{return Err("原始截图长度不符".into())};
 let rgba=image::RgbaImage::from_raw(w,h,bytes[offset..].to_vec()).ok_or("原始截图缓冲区无效")?;
 Ok(image::DynamicImage::ImageRgba8(rgba).to_rgb8())
}
pub(super) fn png(rgb:&image::RgbImage)->Result<Vec<u8>,String>{
 let mut bytes=Vec::new();
 image::codecs::png::PngEncoder::new_with_quality(&mut bytes,image::codecs::png::CompressionType::Fast,image::codecs::png::FilterType::Sub)
  .write_image(rgb.as_raw(),rgb.width(),rgb.height(),image::ExtendedColorType::Rgb8).map_err(|e|e.to_string())?;
 Ok(bytes)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn raw_headers_preserve_pixels_and_reject_truncation(){
  for header in [12,16]{
   let mut b=Vec::new();for v in [2u32,1,1]{b.extend(v.to_le_bytes());}if header==16{b.extend(1u32.to_le_bytes());}
   b.extend([255,0,0,255,0,20,255,255]);let rgb=parse(&b).unwrap();assert_eq!(rgb.as_raw(),&vec![255,0,0,0,20,255]);
   assert_eq!(image::load_from_memory(&png(&rgb).unwrap()).unwrap().to_rgb8(),rgb);
   b.pop();assert!(parse(&b).is_err());
  }
  assert!(parse(&[0;12]).is_err());
 }
}
