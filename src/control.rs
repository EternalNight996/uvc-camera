//! UVC 控制项（对齐 AndroidUSBCamera / saki4510t 的 UVC 控制单元）
//!
//! 覆盖常用控制：曝光、增益、亮度、对比度、饱和度、清晰度、对焦、白平衡。

use nokhwa::utils::{CameraControl, KnownCameraControl};

/// 暴露给上层使用的 UVC 控制项（`KnownCameraControl` 的常用子集）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Brightness,
    Contrast,
    Hue,
    Saturation,
    Sharpness,
    Gamma,
    WhiteBalance,
    BacklightComp,
    Gain,
    Zoom,
    Exposure,
    Focus,
}

impl std::fmt::Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// 控制项当前值（对齐 `ControlValueSetter` 的常用变体）
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

impl ControlValue {
    /// 转为 nokhwa 的 setter 值
    pub(crate) fn into_setter(self) -> nokhwa::utils::ControlValueSetter {
        match self {
            ControlValue::Integer(v) => nokhwa::utils::ControlValueSetter::Integer(v),
            ControlValue::Float(v) => nokhwa::utils::ControlValueSetter::Float(v),
            ControlValue::Boolean(v) => nokhwa::utils::ControlValueSetter::Boolean(v),
        }
    }

    /// 从 nokhwa 的当前值还原
    pub(crate) fn from_setter(setter: nokhwa::utils::ControlValueSetter) -> Self {
        match setter {
            nokhwa::utils::ControlValueSetter::Integer(v) => ControlValue::Integer(v),
            nokhwa::utils::ControlValueSetter::Float(v) => ControlValue::Float(v),
            nokhwa::utils::ControlValueSetter::Boolean(v) => ControlValue::Boolean(v),
            _ => ControlValue::Integer(0),
        }
    }
}

/// 单个控制项的完整信息（名称/当前值/范围/步进/默认值）
#[derive(Clone, Debug)]
pub struct ControlInfo {
    pub control: Control,
    pub name: String,
    pub value: ControlValue,
    /// (min, max)
    pub range: Option<(i64, i64)>,
    pub step: Option<i64>,
    pub default: Option<i64>,
}

impl ControlInfo {
    /// 从 nokhwa `CameraControl` 构建（仅整型范围常用；浮点/布尔作简化处理）
    pub(crate) fn new(control: Control, cc: &CameraControl) -> Self {
        use nokhwa::utils::ControlValueDescription;
        let value = ControlValue::from_setter(cc.value());
        let (range, step, default) = match cc.description() {
            ControlValueDescription::IntegerRange {
                min,
                max,
                step,
                default,
                ..
            } => (Some((*min, *max)), Some(*step), Some(*default)),
            ControlValueDescription::Integer {
                step, default, ..
            } => (None, Some(*step), Some(*default)),
            ControlValueDescription::FloatRange {
                min, max, step, default, ..
            } => (Some((*min as i64, *max as i64)), Some(*step as i64), Some(*default as i64)),
            _ => (None, None, None),
        };
        Self {
            control,
            name: cc.name().to_string(),
            value,
            range,
            step,
            default,
        }
    }
}

/// Control → KnownCameraControl 映射（内部）
pub(crate) fn to_known_control(control: Control) -> Option<KnownCameraControl> {
    Some(match control {
        Control::Brightness => KnownCameraControl::Brightness,
        Control::Contrast => KnownCameraControl::Contrast,
        Control::Hue => KnownCameraControl::Hue,
        Control::Saturation => KnownCameraControl::Saturation,
        Control::Sharpness => KnownCameraControl::Sharpness,
        Control::Gamma => KnownCameraControl::Gamma,
        Control::WhiteBalance => KnownCameraControl::WhiteBalance,
        Control::BacklightComp => KnownCameraControl::BacklightComp,
        Control::Gain => KnownCameraControl::Gain,
        Control::Zoom => KnownCameraControl::Zoom,
        Control::Exposure => KnownCameraControl::Exposure,
        Control::Focus => KnownCameraControl::Focus,
    })
}

/// KnownCameraControl → Control 反向映射（过滤掉不常用的 `Other`）
pub(crate) fn from_known_control(known: KnownCameraControl) -> Option<Control> {
    Some(match known {
        KnownCameraControl::Brightness => Control::Brightness,
        KnownCameraControl::Contrast => Control::Contrast,
        KnownCameraControl::Hue => Control::Hue,
        KnownCameraControl::Saturation => Control::Saturation,
        KnownCameraControl::Sharpness => Control::Sharpness,
        KnownCameraControl::Gamma => Control::Gamma,
        KnownCameraControl::WhiteBalance => Control::WhiteBalance,
        KnownCameraControl::BacklightComp => Control::BacklightComp,
        KnownCameraControl::Gain => Control::Gain,
        KnownCameraControl::Zoom => Control::Zoom,
        KnownCameraControl::Exposure => Control::Exposure,
        KnownCameraControl::Focus => Control::Focus,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_roundtrip_all_known() {
        for c in [
            Control::Brightness,
            Control::Contrast,
            Control::Hue,
            Control::Saturation,
            Control::Sharpness,
            Control::Gamma,
            Control::WhiteBalance,
            Control::BacklightComp,
            Control::Gain,
            Control::Zoom,
            Control::Exposure,
            Control::Focus,
        ] {
            let known = to_known_control(c).unwrap();
            assert_eq!(from_known_control(known), Some(c), "roundtrip {c}");
        }
    }

    #[test]
    fn control_value_setter_roundtrip() {
        let v = ControlValue::Integer(42);
        assert_eq!(ControlValue::from_setter(v.into_setter()), v);
        let v = ControlValue::Float(3.5);
        assert_eq!(ControlValue::from_setter(v.into_setter()), v);
        let v = ControlValue::Boolean(true);
        assert_eq!(ControlValue::from_setter(v.into_setter()), v);
    }

    #[test]
    fn unknown_control_maps_to_none() {
        use nokhwa::utils::KnownCameraControl::Other;
        assert_eq!(from_known_control(Other(1234)), None);
    }
}
