//! 设备枚举与打开选项（对齐 AndroidUSBCamera 的 `USBMonitor` + `DeviceFilter`）

use crate::Result;

/// 一个 UVC 相机设备
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    /// 设备序号（传给 [`crate::UvcCamera::open`]）
    pub index: u32,
    /// 设备可读名称（Windows 上通常含厂商 + VID/PID 信息）
    pub name: String,
    /// 设备描述
    pub description: String,
    /// 设备 misc 信息（平台相关，可能含 VID/PID）
    pub misc: String,
}

impl DeviceInfo {
    /// 从 nokhwa `CameraInfo` 转换
    pub(crate) fn from_camera_info(info: &nokhwa::utils::CameraInfo) -> Self {
        let index = info.index().as_index().unwrap_or(0);
        Self {
            index,
            name: info.human_name(),
            description: info.description().to_string(),
            misc: info.misc(),
        }
    }
}

/// 打开相机时的分辨率/帧率偏好（对齐 `CameraRequest.Builder`）
#[derive(Clone, Debug)]
pub struct OpenOptions {
    /// 请求宽度（None = 使用最高分辨率）
    pub width: Option<u32>,
    /// 请求高度（None = 使用最高分辨率）
    pub height: Option<u32>,
    /// 请求帧率（默认 30）
    pub fps: Option<u32>,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            fps: Some(30),
        }
    }
}

impl OpenOptions {
    /// 指定分辨率（便捷构造）
    pub fn with_resolution(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// 指定帧率（便捷构造）
    pub fn with_fps(mut self, fps: u32) -> Self {
        self.fps = Some(fps);
        self
    }
}

/// 枚举系统内所有 UVC 相机（免驱设备，即插即用）
///
/// 等价 AndroidUSBCamera 的 `USBMonitor` + 多相机选择；返回空 Vec 表示无相机。
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let infos = nokhwa::query(nokhwa::utils::ApiBackend::Auto)?;
    Ok(infos.iter().map(DeviceInfo::from_camera_info).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_options_default_uses_highest_resolution() {
        let opts = OpenOptions::default();
        assert_eq!(opts.width, None);
        assert_eq!(opts.fps, Some(30));
        // 未指定宽高时走最高分辨率（不 panic 即为通过）
        let requested = match (opts.width, opts.height) {
            (Some(_), Some(_)) => "exact",
            _ => "highest",
        };
        assert_eq!(requested, "highest");
    }

    #[test]
    fn open_options_with_resolution_sets_width_height() {
        let opts = OpenOptions::default().with_resolution(1280, 720).with_fps(15);
        assert_eq!(opts.width, Some(1280));
        assert_eq!(opts.height, Some(720));
        assert_eq!(opts.fps, Some(15));
    }
}
