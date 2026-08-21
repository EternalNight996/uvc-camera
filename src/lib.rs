//! # uvc-camera
//!
//! 跨平台 UVC 相机统一引擎（基于 [nokhwa]）：
//!
//! - **Windows**：MediaFoundation（免驱即插即用，UVC 免驱工业相机的标准接入方式）
//! - **Linux**：V4L2
//! - **macOS**：AVFoundation
//!
//! API 设计参考 AndroidUSBCamera（jiangdongguo）高层引擎的 `CameraClient` 形态：
//! 设备枚举 → 打开（分辨率/帧率）→ 后台预览流 → 单帧抓图 → UVC 控制（曝光/增益/亮度…）。
//!
//! 打开相机即自动启动后台预览线程（nokhwa `CallbackCamera`），
//! [`UvcCamera::frame`] 非阻塞取最新帧，[`UvcCamera::capture`] 阻塞取最新一帧用于拍照。
//!
//! ```rust,no_run
//! use uvc_camera::{list_devices, OpenOptions, UvcCamera};
//!
//! let devs = list_devices()?;
//! if let Some(dev) = devs.first() {
//!     let mut cam = UvcCamera::open(dev.index, OpenOptions::default())?;
//!     let frame = cam.frame()?;            // 最新帧（RGBA，预览用）
//!     let jpeg = cam.capture(85)?;         // 阻塞取一帧 → JPEG（拍照用）
//! }
//! # Ok::<(), uvc_camera::Error>(())
//! ```

use std::sync::{Arc, Mutex};

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::CallbackCamera;

pub mod control;
pub mod device;

pub use control::{Control, ControlInfo, ControlValue};
pub use device::{list_devices, DeviceInfo, OpenOptions};
pub use nokhwa::utils::{CameraFormat, FrameFormat, Resolution};

use crate::control::{from_known_control, to_known_control};

/// 统一错误类型：透传 nokhwa / 图像编码错误
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("相机错误: {0}")]
    Camera(#[from] nokhwa::NokhwaError),
    #[error("图像编码错误: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Other(String),
}

/// 跨平台统一返回
pub type Result<T> = std::result::Result<T, Error>;

/// UVC 相机句柄（对齐 AndroidUSBCamera 的 `CameraClient`）
///
/// 内部为 nokhwa `CallbackCamera`：打开即启动后台取帧线程，帧数据持续缓存，
/// `frame()` 非阻塞读最新帧。
pub struct UvcCamera {
    cam: CallbackCamera,
    format: CameraFormat,
    /// 最新 RGBA 帧（后台解码线程写入；`frame()` 只读缓存，零解码不阻塞主线程）
    latest_rgba: Arc<Mutex<Option<RgbaFrame>>>,
}

impl UvcCamera {
    /// 按设备序号 + 分辨率/帧率偏好打开相机并启动后台预览
    ///
    /// - `index`：设备序号（[`list_devices`] 返回）
    /// - `options`：分辨率/帧率偏好；未指定宽高时走「最高分辨率」
    pub fn open(index: u32, options: OpenOptions) -> Result<Self> {
        let requested = match (options.width, options.height) {
            (Some(w), Some(h)) => {
                let fmt = CameraFormat::new(
                    Resolution::new(w, h),
                    FrameFormat::MJPEG,
                    options.fps.unwrap_or(30),
                );
                RequestedFormatType::Closest(fmt)
            }
            _ => RequestedFormatType::AbsoluteHighestResolution,
        };
        let req = RequestedFormat::new::<RgbFormat>(requested);
        // 后台线程回调：解码 MJPEG 帧 → RGBA 缓存（避免 GUI 主线程做 CPU 密集解码导致卡顿）
        let latest_rgba = Arc::new(Mutex::new(None::<RgbaFrame>));
        let latest_clone = latest_rgba.clone();
        let mut cam = CallbackCamera::new(CameraIndex::Index(index), req, move |buf| {
            if let Ok(img) = buf.decode_image::<RgbFormat>() {
                let (w, h) = img.dimensions();
                let frame = rgb_to_rgba(w, h, img.into_raw());
                if let Ok(mut l) = latest_clone.lock() {
                    *l = Some(frame);
                }
            }
        })?;
        cam.open_stream()?;
        let format = cam.camera_format()?;
        Ok(Self { cam, format, latest_rgba })
    }

    /// 当前生效的采集格式（分辨率 / 帧率）
    pub fn format(&self) -> CameraFormat {
        self.format
    }

    /// 当前分辨率 (width, height)
    pub fn resolution(&self) -> (u32, u32) {
        (self.format.resolution().width_x, self.format.resolution().height_y)
    }

    /// 枚举相机支持的格式（UVC 协议：分辨率 × 帧率 × 像素格式组合）
    ///
    /// 用于设置页「分辨率下拉框」——严格按相机实际支持项展示，
    /// 而非预置列表（遵循 UVC 协议）。
    ///
    /// 实现：遍历相机支持的像素格式（fourcc），对每种格式取分辨率→帧率表，
    /// 组合成完整 CameraFormat 列表。
    pub fn camera_formats(&mut self) -> Result<Vec<CameraFormat>> {
        let mut out = Vec::new();
        // 1. 相机支持的像素格式（MJPEG / YUYV 等）
        let fourccs = self.cam.compatible_fourcc().map_err(Error::Camera)?;
        // 2. 每种格式：分辨率 → 帧率列表
        for fourcc in fourccs {
            let map = self
                .cam
                .compatible_list_by_resolution(fourcc)
                .map_err(Error::Camera)?;
            for (res, fps_list) in map {
                for fps in fps_list {
                    out.push(CameraFormat::new(res, fourcc, fps));
                }
            }
        }
        Ok(out)
    }

    /// 切换分辨率（Closest 策略）
    pub fn set_resolution(&mut self, width: u32, height: u32, fps: u32) -> Result<()> {
        let req = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
            CameraFormat::new(
                Resolution::new(width, height),
                FrameFormat::MJPEG,
                fps,
            ),
        ));
        self.format = self.cam.set_camera_requset(req)?;
        Ok(())
    }

    /// 取最新一帧 RGBA（非阻塞；后台线程已解码缓存，主线程零解码）
    ///
    /// 打开即由回调线程持续解码 MJPEG → RGBA；此处仅读缓存（浅拷贝 Vec），
    /// 不阻塞 GUI 主线程，保证预览流畅。
    pub fn frame(&mut self) -> Result<RgbaFrame> {
        let l = self
            .latest_rgba
            .lock()
            .map_err(|e| Error::Other(format!("帧缓存锁失败: {e}")))?;
        match &*l {
            Some(f) => Ok(f.clone()),
            None => Err(Error::Other("相机尚未产生帧（后台解码中）".into())),
        }
    }

    /// 阻塞取一帧并编码为 JPEG（对齐 AndroidUSBCamera 的 JPG 拍照）
    ///
    /// `quality`：JPEG 质量 1..=100
    pub fn capture(&mut self, quality: u8) -> Result<Vec<u8>> {
        let buf = self.cam.poll_frame()?;
        let img = buf.decode_image::<RgbFormat>()?;
        let mut out = std::io::Cursor::new(Vec::new());
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.min(100));
        img.write_with_encoder(encoder)?;
        Ok(out.into_inner())
    }

    /// 停止预览线程并关闭相机
    pub fn close(&mut self) -> Result<()> {
        self.cam.stop_stream()?;
        Ok(())
    }

    /// 支持的 UVC 控制项列表（含当前值/范围，对齐 `getSupportedControls`）
    pub fn supported_controls(&mut self) -> Result<Vec<ControlInfo>> {
        let controls = self.cam.camera_controls()?;
        Ok(controls
            .into_iter()
            .filter_map(|cc| from_known_control(cc.control()).map(|c| ControlInfo::new(c, &cc)))
            .collect())
    }

    /// 设置 UVC 控制项（对齐 `setCtrlValue`）
    pub fn set_control(&mut self, control: Control, value: ControlValue) -> Result<()> {
        let known = to_known_control(control).ok_or_else(|| Error::Other("不支持的控件".into()))?;
        self.cam.set_camera_control(known, value.into_setter())?;
        Ok(())
    }

    /// 读取单个控制项当前值
    pub fn get_control(&mut self, control: Control) -> Result<ControlValue> {
        let known = to_known_control(control).ok_or_else(|| Error::Other("不支持的控件".into()))?;
        let cc = self
            .cam
            .camera_controls()?
            .into_iter()
            .find(|c| c.control() == known)
            .ok_or_else(|| Error::Other(format!("控件不可用: {control}")))?;
        Ok(ControlValue::from_setter(cc.value()))
    }
}

/// 解码一帧 Buffer 为 RGBA 像素（RGB → RGBA，alpha=255，兼容 egui 纹理）
fn decode_to_rgba(buf: nokhwa::Buffer) -> Result<RgbaFrame> {
    let img = buf.decode_image::<RgbFormat>()?;
    let (w, h) = img.dimensions();
    Ok(rgb_to_rgba(w, h, img.into_raw()))
}

/// RGB 原始数据 → RGBA 帧（alpha 固定 255）
fn rgb_to_rgba(width: u32, height: u32, raw: Vec<u8>) -> RgbaFrame {
    RgbaFrame {
        width,
        height,
        data: raw
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
    }
}

/// 一帧 RGBA 像素（width × height × 4 bytes）
#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// 原生后端信息（跨平台常量，调试/日志用）
pub fn native_backend_name() -> &'static str {
    match nokhwa::native_api_backend() {
        Some(ApiBackend::MediaFoundation) => "Windows MediaFoundation",
        Some(ApiBackend::Video4Linux) => "Linux V4L2",
        Some(ApiBackend::AVFoundation) => "macOS AVFoundation",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_name_is_platform_specific() {
        // 至少返回非空字符串（不 panic 即通过；Windows 下应为 MediaFoundation）
        let name = native_backend_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn rgba_decode_shape() {
        // 2x1 像素 RGB → RGBA：验证 alpha 填充为 255
        let frame = rgb_to_rgba(2, 1, vec![255, 0, 0, 0, 255, 0]);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.data.len(), 2 * 1 * 4);
        assert_eq!(&frame.data[0..4], &[255, 0, 0, 255]);
        assert_eq!(&frame.data[4..8], &[0, 255, 0, 255]);
    }
}
