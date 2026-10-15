//! MuMu renderer capture. Bind an instance by Android boot UUID, never by serial spelling.
use super::{Adb, CancelSignal, raw_frame};
use opencv::{core, imgproc, prelude::*};
use std::{path::PathBuf, sync::{Arc, Mutex}, time::{Duration, Instant}};

pub struct Capture { inner: Option<Arc<Mutex<Native>>>, pub status: String }
type Connect = unsafe extern "C" fn(*const u16, i32) -> i32;
type Disconnect = unsafe extern "C" fn(i32);
type Read = unsafe extern "C" fn(i32,u32,i32,*mut i32,*mut i32,*mut u8)->i32;
struct Native { _library: libloading::Library, handle:i32, read:Read, disconnect:Disconnect }
impl Drop for Native { fn drop(&mut self) { unsafe { (self.disconnect)(self.handle) }; } }

async fn command(path: &std::path::Path,args:&[&str],cancel:&CancelSignal)->Result<Vec<u8>,String>{
    let mut command=tokio::process::Command::new(path);
    command.args(args).kill_on_drop(true);
    #[cfg(windows)] command.creation_flags(0x08000000);
    tokio::select! { biased;
        _=cancel.cancelled()=>Err("操作已取消".into()),
        result=tokio::time::timeout(Duration::from_secs(5),command.output())=>{
            let out=result.map_err(|_|"MuMu 查询超时")?.map_err(|e|e.to_string())?;
            if !out.status.success() {return Err("MuMu 查询失败".into());}
            Ok(out.stdout)
        }
    }
}
fn boot_id(bytes:&[u8])->Option<String>{
    let s=std::str::from_utf8(bytes).ok()?.trim();
    uuid::Uuid::parse_str(s).ok().filter(|id|!id.is_nil()).map(|id|id.to_string())
}
impl Native {
    fn open(root:PathBuf,index:i32)->Result<Self,String>{
        let dll=root.join("nx_main/sdk/external_renderer_ipc.dll");
        // Load only the SDK belonging to the running MuMu installation.
        unsafe {
            let library=libloading::Library::new(dll).map_err(|e|e.to_string())?;
            let connect:Connect=*library.get(b"nemu_connect\0").map_err(|e|e.to_string())?;
            let read:Read=*library.get(b"nemu_capture_display\0").map_err(|e|e.to_string())?;
            let disconnect:Disconnect=*library.get(b"nemu_disconnect\0").map_err(|e|e.to_string())?;
            let path:Vec<u16>=root.to_string_lossy().encode_utf16().chain(Some(0)).collect();
            let handle=connect(path.as_ptr(),index);
            if handle<=0 {return Err("MuMu IPC 连接失败".into());}
            Ok(Self{_library:library,handle,read,disconnect})
        }
    }
    fn frame(&mut self)->Result<image::RgbImage,String>{
        let(mut w,mut h)=(0,0);
        let result=unsafe{(self.read)(self.handle,0,0,&mut w,&mut h,std::ptr::null_mut())};
        let count=buffer_size(w,h)?;
        if result!=0 {return Err(format!("MuMu 截图尺寸查询失败 {result}"));}
        let original=(w,h);let mut pixels=vec![0u8;count];
        let result=unsafe{(self.read)(self.handle,0,count as i32,&mut w,&mut h,pixels.as_mut_ptr())};
        if result!=0 || (w,h)!=original {return Err("MuMu 截图失败或分辨率变化".into());}
        rgba_bottom_up(w as u32,h as u32,&pixels)
    }
}
fn buffer_size(w:i32,h:i32)->Result<usize,String>{
    if w<=0||h<=0||w>8192||h>8192{return Err("MuMu 截图尺寸无效".into());}
    let n=w as usize*h as usize*4;
    if n>64*1024*1024{return Err("MuMu 截图过大".into());}Ok(n)
}
fn rgba_bottom_up(w:u32,h:u32,bytes:&[u8])->Result<image::RgbImage,String>{
    if bytes.len()!=buffer_size(w as i32,h as i32)?{return Err("MuMu 像素长度不符".into());}
    let convert=(||->opencv::Result<image::RgbImage>{
        let pixels=Mat::from_slice(bytes)?;let rgba=pixels.reshape(4,h as i32)?;
        let mut rgb=Mat::default();imgproc::cvt_color_def(&rgba,&mut rgb,imgproc::COLOR_RGBA2RGB)?;
        let mut upright=Mat::default();core::flip(&rgb,&mut upright,0)?;
        Ok(image::RgbImage::from_raw(w,h,upright.data_bytes()?.to_vec()).expect("contiguous RGB frame"))
    })();convert.map_err(|e|e.to_string())
}
impl Capture {
    pub async fn discover(adb:&Adb,serial:&str,cancel:&CancelSignal)->Self{
        let result=Self::connect(adb,serial,cancel).await;
        match result {
            Ok(native)=>Self{inner:Some(Arc::new(Mutex::new(native))),status:"mumu-ipc：Android boot UUID 与 ADB 画面校验通过".into()},
            Err(e)=>Self{inner:None,status:format!("使用 ADB raw：{e}")},
        }
    }
    async fn connect(adb:&Adb,serial:&str,cancel:&CancelSignal)->Result<Native,String>{
        if !cfg!(windows){return Err("非 Windows 平台".into());}
        let paths=command(std::path::Path::new("powershell.exe"),&["-NoProfile","-NonInteractive","-Command","[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; Get-Process MuMuNxMain -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path -Unique"],cancel).await?;
        let roots:Vec<_>=String::from_utf8_lossy(&paths).lines().filter_map(|p|PathBuf::from(p.trim()).parent()?.parent().map(|p|p.to_path_buf())).collect();
        let current=boot_id(&adb.execute(&["-s",serial,"shell","cat","/proc/sys/kernel/random/boot_id"],cancel).await?).ok_or("ADB boot UUID 无效")?;
        for root in roots {
            let info=command(&root.join("nx_main/MuMuManager.exe"),&["info","-v","all"],cancel).await?;
            let info:serde_json::Value=serde_json::from_slice(&info).map_err(|e|e.to_string())?;
            for (index,item) in info.as_object().ok_or("MuMu 实例列表无效")? {
                if item["is_android_started"]!=true {continue;}
                let port=item["adb_port"].as_u64().filter(|p|*p>0&&*p<=65535).ok_or("MuMu ADB 端口无效")?;
                let endpoint=format!("127.0.0.1:{port}");
                // Connecting a transport is read-only; taps remain on the user's selected serial.
                adb.execute(&["connect",&endpoint],cancel).await?;
                let candidate=adb.execute(&["-s",&endpoint,"shell","cat","/proc/sys/kernel/random/boot_id"],cancel).await?;
                if boot_id(&candidate).as_deref()!=Some(&current) {continue;}
                let index=index.parse::<i32>().map_err(|e|e.to_string())?;
                let root=root.clone();
                let task=tokio::task::spawn_blocking(move||{let mut native=Native::open(root,index)?;let frame=native.frame()?;Ok::<_,String>((native,frame))});
                let (native,frame)=tokio::time::timeout(Duration::from_secs(2),task).await.map_err(|_|"MuMu IPC 初始化超时")?.map_err(|e|e.to_string())??;
                let (reference,_,_)=adb.screenshot_fast(serial,cancel).await?;
                if !same_screen(&frame,&reference){return Err("IPC 与 ADB 画面不一致，保留 ADB 截图".into());}
                return Ok(native);
            }
        }Err("未找到与当前设备匹配的 MuMu 实例".into())
    }
    pub async fn frame(&mut self,cancel:&CancelSignal)->Option<(image::RgbImage,Vec<u8>,f64,f64)>{
        let inner=self.inner.as_ref()?.clone();
        let task=tokio::task::spawn_blocking(move||{
            let start=Instant::now();let rgb=inner.lock().map_err(|e|e.to_string())?.frame()?;
            let capture_ms=start.elapsed().as_secs_f64()*1000.;let start=Instant::now();
            let png=raw_frame::png(&rgb)?;Ok::<_,String>((rgb,png,capture_ms,start.elapsed().as_secs_f64()*1000.))
        });
        let result=tokio::select!{biased;_=cancel.cancelled()=>Err("取消".to_string()),r=tokio::time::timeout(Duration::from_secs(2),task)=>r.map_err(|_|"IPC 超时".to_string()).and_then(|r|r.map_err(|e|e.to_string())).and_then(|r|r)};
        match result{Ok(frame)=>Some(frame),Err(e)=>{self.inner=None;self.status=format!("IPC 已停用，回退 ADB：{e}");None}}
    }
}
fn same_screen(a:&image::RgbImage,b:&image::RgbImage)->bool{
    if a.dimensions()!=b.dimensions(){return false;}
    let a=image::imageops::resize(a,64,36,image::imageops::FilterType::Triangle);
    let b=image::imageops::resize(b,64,36,image::imageops::FilterType::Triangle);
    let error:f64=a.as_raw().iter().zip(b.as_raw()).map(|(a,b)|(*a as f64-*b as f64).abs()).sum();
    let (min,max)=a.as_raw().iter().fold((255u8,0u8),|(min,max),&v|(min.min(v),max.max(v)));
    max-min>30 && error/(64.*36.*3.)<12.
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_pixels_keep_rgb_and_flip_rows(){let rgb=rgba_bottom_up(1,2,&[255,0,0,255,0,0,255,255]).unwrap();assert_eq!(rgb.as_raw(),&[0,0,255,255,0,0]);assert!(rgba_bottom_up(2,2,&[0;4]).is_err());assert!(buffer_size(8192,8192).is_err());assert!(buffer_size(0,1).is_err());}
    #[test]fn reject_wrong_device_and_blank_screen(){assert!(boot_id(b"garbage").is_none());assert!(boot_id(b"00000000-0000-0000-0000-000000000000").is_none());let a=image::RgbImage::new(64,36);assert!(!same_screen(&a,&a));assert!(!same_screen(&a,&image::RgbImage::new(32,18)));}
    #[test]fn screen_check_accepts_matching_content_rejects_flipped_content(){
        let a=image::RgbImage::from_fn(64,36,|_,y|if y<18{image::Rgb([255,0,0])}else{image::Rgb([0,0,255])});
        assert!(same_screen(&a,&a));assert!(!same_screen(&a,&image::imageops::flip_vertical(&a)));
    }
}
