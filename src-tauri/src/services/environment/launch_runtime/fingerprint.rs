use serde_json::Value;

use crate::infrastructure::runtime::FingerprintConfig;

use super::gpu_ids::GPU_DEVICE_IDS;
use super::kernel::get_window_info;

pub(super) fn build_fingerprint_config(config: Option<&Value>) -> FingerprintConfig {
    let config = build_fingerprint_config_inner(config);
    #[cfg(target_os = "linux")]
    let config = with_server_gpu_defaults(config, std::env::var("MZ_BROWSER_GPU").ok());
    config
}

/// Linux 服务器默认用 SwiftShader 软件渲染，真实的 WebGL 厂商/渲染器是
/// "Google Inc. (Google) / ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device ...))"，
/// 一眼就是无头/虚拟机。环境没有自己设置显卡信息时，换成常见的 Linux 核显（Mesa）。
/// 用真实 GPU（MZ_BROWSER_GPU=1）时不动。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn with_server_gpu_defaults(mut config: FingerprintConfig, gpu: Option<String>) -> FingerprintConfig {
    let real_gpu = gpu.is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes"));
    let unset = config.webgl_vendor.as_deref().map_or(true, str::is_empty)
        && config.webgl_renderer.as_deref().map_or(true, str::is_empty);
    if real_gpu || !unset || config.webgl_info.as_deref() == Some("real") {
        return config;
    }
    config.webgl_info = Some("custom".to_string());
    config.webgl_vendor = Some(SERVER_WEBGL_VENDOR.to_string());
    config.webgl_renderer = Some(SERVER_WEBGL_RENDERER.to_string());
    if config.webgpu.is_none() {
        config.webgpu = Some("webgl-match".to_string());
    }
    config
}

const SERVER_WEBGL_VENDOR: &str = "Google Inc. (Intel)";
const SERVER_WEBGL_RENDERER: &str = "ANGLE (Intel, Mesa Intel(R) UHD Graphics 630 (CFL GT2), OpenGL 4.6)";

fn build_fingerprint_config_inner(config: Option<&Value>) -> FingerprintConfig {
    let window_info = with_host_identity(get_window_info(config));
    let basic_settings = get_object(config, "basic_settings");
    let fingerprint_settings = get_object(config, "fingerprint_settings");
    let device_settings = get_object(config, "device_settings");
    let preference_settings = get_object(config, "preference_settings");

    FingerprintConfig {
        language: get_string(&basic_settings, "language"),
        interface_language: get_string(&basic_settings, "interfaceLanguage"),
        timezone: get_string(&basic_settings, "timezone"),
        geolocation_prompt: get_string(&basic_settings, "geolocationPrompt"),
        geolocation: get_json_string(&basic_settings, "geolocation"),
        platform: get_string_from_map(&window_info, "system"),
        platform_version: platform_version_for(&window_info),
        user_agent: get_string_from_map(&window_info, "userAgent"),
        // 创建时定下的 Client Hints 完整版本号；启动时再核对一次主版本（见 launch_runtime/mod.rs）
        original_chrome_version: get_string_from_map(&window_info, "uaFullVersion"),
        sound: get_bool(&basic_settings, "sound"),
        images: get_bool(&basic_settings, "images"),
        video: get_bool(&basic_settings, "video"),
        window_size: get_string(&basic_settings, "windowSize"),
        window_width: get_i32(&basic_settings, "windowWidth"),
        window_height: get_i32(&basic_settings, "windowHeight"),
        window_position: get_string(&basic_settings, "windowPosition"),
        window_x: get_i32(&basic_settings, "windowX"),
        window_y: get_i32(&basic_settings, "windowY"),
        resolution: get_value(&fingerprint_settings, "resolution"),
        color_depth: get_i32(&fingerprint_settings, "colorDepth"),
        device_pixel_ratio: get_f64(&fingerprint_settings, "devicePixelRatio"),
        max_touch_points: get_i32(&fingerprint_settings, "maxTouchPoints"),
        canvas: get_string(&fingerprint_settings, "canvas"),
        webgl_image: get_string(&fingerprint_settings, "webglImage"),
        webgl_info: get_string(&fingerprint_settings, "webglInfo"),
        webgl_vendor: get_string(&fingerprint_settings, "webglVendor"),
        webgl_renderer: get_string(&fingerprint_settings, "webglRenderer").map(|renderer| with_device_id(&renderer)),
        webgpu: get_string(&fingerprint_settings, "webgpu"),
        font_fingerprint: get_string(&fingerprint_settings, "fontFingerprint"),
        font_list: with_locale_fonts(
            get_value(&fingerprint_settings, "fontList"),
            get_string(&basic_settings, "language").as_deref(),
            window_info.get("system").and_then(Value::as_str),
        ),
        audio_context: get_string(&fingerprint_settings, "audioContext"),
        speech_voices: get_string(&fingerprint_settings, "speechVoices"),
        client_rects: get_string(&fingerprint_settings, "clientRects"),
        media_devices: get_string(&fingerprint_settings, "mediaDevices"),
        webrtc: get_string(&fingerprint_settings, "webrtc"),
        do_not_track: get_bool(&fingerprint_settings, "doNotTrack"),
        device_name: get_string(&device_settings, "deviceName"),
        device_name_random: get_bool(&device_settings, "deviceNameRandom"),
        mac_address: get_string(&device_settings, "macAddress"),
        mac_address_mode: get_string(&device_settings, "macAddressMode"),
        hardware_concurrency: get_i32(&device_settings, "hardwareConcurrency"),
        device_memory: get_i32(&device_settings, "deviceMemory"),
        ssl_fingerprint: get_bool(&device_settings, "sslFingerprint"),
        port_scan_protection: get_bool(&device_settings, "portScanProtection"),
        scan_whitelist: get_string(&device_settings, "scanWhitelist"),
        hardware_acceleration: get_bool(&device_settings, "hardwareAcceleration"),
        disable_sandbox: get_bool(&device_settings, "disableSandbox"),
        startup_parameters: get_string(&device_settings, "startupParameters"),
        random_fingerprint_on_launch: get_bool(&preference_settings, "randomFingerprintOnLaunch"),
        env_id: None,
        env_name: None,
    }
}

/// 各语言版 Windows 默认自带、且网页常拿来判断系统语言的字体。
/// 内核只暴露白名单里的系统字体；日文环境却查不到 Yu Gothic UI / Meiryo，
/// 而 system-ui（内核按环境语言换成 Yu Gothic UI）又在用它，等于自相矛盾。
/// 中文字体是否暴露仍由 includeChinese 决定，这里不动。
fn locale_windows_fonts(language: &str) -> &'static [&'static str] {
    let language = language.to_ascii_lowercase();
    if language.starts_with("ja") {
        &[
            "Yu Gothic UI",
            "Yu Gothic",
            "Yu Mincho",
            "Meiryo",
            "Meiryo UI",
            "MS Gothic",
            "MS PGothic",
            "MS UI Gothic",
            "MS Mincho",
            "MS PMincho",
            "BIZ UDGothic",
            "BIZ UDPGothic",
            "BIZ UDMincho",
            "BIZ UDPMincho",
            "游ゴシック",
            "游明朝",
            "メイリオ",
            "ＭＳ ゴシック",
            "ＭＳ Ｐゴシック",
            "ＭＳ 明朝",
            "ＭＳ Ｐ明朝",
        ]
    } else if language.starts_with("ko") {
        &[
            "Malgun Gothic",
            "맑은 고딕",
            "Gulim",
            "GulimChe",
            "Dotum",
            "DotumChe",
            "Batang",
            "BatangChe",
            "Gungsuh",
            "GungsuhChe",
            "굴림",
            "돋움",
            "바탕",
            "궁서",
        ]
    } else if language.starts_with("th") {
        &["Leelawadee UI", "Leelawadee", "Tahoma"]
    } else {
        &[]
    }
}

/// 字体列表是 random / ua-match 模式（内核按白名单过滤）时，按环境语言补上该语言 Windows 的默认字体。
/// 只对 Windows 身份生效；system 模式不过滤，不需要补。
fn with_locale_fonts(font_list: Option<Value>, language: Option<&str>, system: Option<&str>) -> Option<Value> {
    let mut font_list = font_list?;
    if matches!(system, Some("macOS") | Some("Linux")) {
        return Some(font_list);
    }
    let extra = locale_windows_fonts(language.unwrap_or_default());
    if extra.is_empty() {
        return Some(font_list);
    }
    let Some(obj) = font_list.as_object_mut() else {
        return Some(font_list);
    };
    if obj.get("mode").and_then(Value::as_str) == Some("system") {
        return Some(font_list);
    }
    if let Some(Value::Array(fonts)) = obj.get_mut("fonts") {
        for font in extra {
            let exists = fonts
                .iter()
                .any(|f| f.as_str().is_some_and(|name| name.eq_ignore_ascii_case(font)));
            if !exists {
                fonts.push(Value::String((*font).to_string()));
            }
        }
    }
    Some(font_list)
}

fn get_object(source: Option<&Value>, key: &str) -> Option<serde_json::Map<String, Value>> {
    source
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_object)
        .cloned()
}

fn get_string(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<String> {
    source
        .as_ref()
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Sec-CH-UA-Platform-Version 的 Windows 取值（见界面的 device-samples.ts）
const WINDOWS_PLATFORM_VERSIONS: [&str; 4] = ["10.0.0", "13.0.0", "15.0.0", "19.0.0"];
/// 非 Windows 身份没有存自己系统的版本号时用：macOS 15 Sequoia、Linux 内核 6.8
const DEFAULT_MACOS_PLATFORM_VERSION: &str = "15.7.1";
const DEFAULT_LINUX_PLATFORM_VERSION: &str = "6.8.0";

/// 环境存的版本号都按 Windows 生成。macOS / Linux 身份不能用它，也不能不发：
/// 不发时内核会报宿主机的 Windows 版本，和身份矛盾。
/// Fork 新增：MCP / 本地 API 建的环境可能只有名字，没有 window_info.userAgent，
/// 内核会以"缺少 User-Agent"拒绝启动。这时按本机系统补一个标准的 Chrome UA
/// （主版本号启动时再对齐内核，见 launch_runtime/mod.rs），system 没填也一起补。
fn with_host_identity(mut window_info: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let has_ua = window_info.get("userAgent").and_then(Value::as_str).is_some_and(|ua| !ua.trim().is_empty());
    if has_ua {
        return window_info;
    }
    let system = window_info
        .get("system")
        .and_then(Value::as_str)
        .filter(|system| !system.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| host_system().to_string());
    let platform = match system.as_str() {
        "macOS" => "Macintosh; Intel Mac OS X 10_15_7",
        "Linux" => "X11; Linux x86_64",
        _ => "Windows NT 10.0; Win64; x64",
    };
    window_info.insert(
        "userAgent".into(),
        Value::String(format!(
            "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{DEFAULT_UA_MAJOR}.0.0.0 Safari/537.36"
        )),
    );
    window_info.insert("system".into(), Value::String(system));
    window_info
}

const DEFAULT_UA_MAJOR: u32 = 158;

fn host_system() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Windows"
    }
}

fn platform_version_for(window_info: &serde_json::Map<String, Value>) -> Option<String> {
    let stored = get_string_from_map(window_info, "platformVersion").filter(|version| !version.trim().is_empty());
    let own_or = |default: &str| {
        Some(stored.clone().filter(|version| !WINDOWS_PLATFORM_VERSIONS.contains(&version.as_str())).unwrap_or_else(|| default.to_string()))
    };
    match window_info.get("system").and_then(Value::as_str) {
        Some("macOS") => own_or(DEFAULT_MACOS_PLATFORM_VERSION),
        Some("Linux") => own_or(DEFAULT_LINUX_PLATFORM_VERSION),
        _ => stored,
    }
}

fn get_string_from_map(source: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    source.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn get_json_string(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<String> {
    source.as_ref().and_then(|obj| obj.get(key)).map(|value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    })
}

fn get_bool(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<bool> {
    source.as_ref().and_then(|obj| obj.get(key)).and_then(Value::as_bool)
}

fn get_i32(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<i32> {
    source
        .as_ref()
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_i64)
        .map(|value| value as i32)
}

fn get_f64(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<f64> {
    source.as_ref().and_then(|obj| obj.get(key)).and_then(Value::as_f64)
}

fn get_value(source: &Option<serde_json::Map<String, Value>>, key: &str) -> Option<Value> {
    source.as_ref().and_then(|obj| obj.get(key)).cloned()
}

/// 和 gen-gpu-ids.py 里的 norm() 保持一致：小写、去掉 (R)/(TM)、只留字母数字、去掉厂商名和 "series"。
fn normalize_gpu_name(name: &str) -> String {
    let lowered = name.to_lowercase().replace("(r)", "").replace("(tm)", "");
    let mut key: String = lowered.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    for word in ["nvidia", "amd", "intel", "series"] {
        key = key.replace(word, "");
    }
    key
}

/// 新版 Chrome 的 ANGLE 显卡串带 PCI 设备号，如 "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 ...)"。
/// 旧环境里的串没有设备号，这里按型号名补上；型号查不到、串格式不认识或已经带设备号时原样返回。
pub(super) fn with_device_id(renderer: &str) -> String {
    if renderer.contains("(0x") {
        return renderer.to_string();
    }
    let Some(d3d) = renderer.find(" Direct3D11") else {
        return renderer.to_string();
    };
    let Some(comma) = renderer[..d3d].find(", ") else {
        return renderer.to_string();
    };
    let key = normalize_gpu_name(&renderer[comma + 2..d3d]);
    match GPU_DEVICE_IDS.binary_search_by(|(name, _)| (*name).cmp(key.as_str())) {
        Ok(index) => format!("{} (0x{:08X}){}", &renderer[..d3d], GPU_DEVICE_IDS[index].1, &renderer[d3d..]),
        Err(_) => renderer.to_string(),
    }
}

#[cfg(test)]
mod server_gpu_tests {
    use super::{build_fingerprint_config_inner, with_server_gpu_defaults};

    #[test]
    fn unset_gpu_strings_become_a_common_mesa_igpu_on_software_rendering() {
        let config = with_server_gpu_defaults(build_fingerprint_config_inner(None), None);
        assert_eq!(config.webgl_info.as_deref(), Some("custom"));
        assert!(config.webgl_renderer.unwrap().contains("Mesa Intel"));
    }

    #[test]
    fn configured_or_real_gpu_is_left_alone() {
        let config = with_server_gpu_defaults(build_fingerprint_config_inner(None), Some("1".into()));
        assert_eq!(config.webgl_renderer, None);
        let value = serde_json::json!({"fingerprint_settings": {"webglVendor": "V", "webglRenderer": "R", "webglInfo": "custom"}});
        let config = with_server_gpu_defaults(build_fingerprint_config_inner(Some(&value)), None);
        assert_eq!(config.webgl_renderer.as_deref(), Some("R"));
    }
}

#[cfg(test)]
mod device_id_tests {
    use super::{build_fingerprint_config, normalize_gpu_name, with_device_id, GPU_DEVICE_IDS};

    #[test]
    fn every_identity_reports_a_version_of_its_own_system() {
        let version = |system: &str, stored: Option<&str>| {
            let mut window_info = serde_json::json!({ "system": system });
            if let Some(stored) = stored {
                window_info["platformVersion"] = serde_json::json!(stored);
            }
            build_fingerprint_config(Some(&serde_json::json!({ "window_info": window_info }))).platform_version
        };
        assert_eq!(version("Windows", Some("19.0.0")).as_deref(), Some("19.0.0"));
        assert_eq!(version("macOS", Some("19.0.0")).as_deref(), Some("15.7.1"));
        assert_eq!(version("macOS", None).as_deref(), Some("15.7.1"));
        assert_eq!(version("macOS", Some("26.1.0")).as_deref(), Some("26.1.0"));
        assert_eq!(version("Linux", Some("10.0.0")).as_deref(), Some("6.8.0"));
    }

    #[test]
    fn table_is_sorted_for_binary_search() {
        assert!(GPU_DEVICE_IDS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn old_renderer_strings_get_their_device_id() {
        assert_eq!(
            with_device_id("ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)"),
            "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 vs_5_0 ps_5_0, D3D11)"
        );
        assert_eq!(
            with_device_id("ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)"),
            "ANGLE (Intel, Intel(R) UHD Graphics 630 (0x00003E92) Direct3D11 vs_5_0 ps_5_0, D3D11)"
        );
        assert_eq!(
            with_device_id("ANGLE (AMD, AMD Radeon RX 580 Series Direct3D11 vs_5_0 ps_5_0, D3D11)"),
            "ANGLE (AMD, AMD Radeon RX 580 Series (0x000067DF) Direct3D11 vs_5_0 ps_5_0, D3D11)"
        );
        assert_eq!(
            with_device_id("ANGLE (AMD, AMD Radeon(TM) Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)"),
            "ANGLE (AMD, AMD Radeon(TM) Graphics (0x00001638) Direct3D11 vs_5_0 ps_5_0, D3D11)"
        );
    }

    #[test]
    fn unknown_or_already_complete_strings_are_left_alone() {
        let complete = "ANGLE (NVIDIA, NVIDIA GeForce RTX 5070 Ti (0x00002C05) Direct3D11 vs_5_0 ps_5_0, D3D11)";
        assert_eq!(with_device_id(complete), complete);
        let unknown = "ANGLE (NVIDIA, NVIDIA Made Up 9000 Direct3D11 vs_5_0 ps_5_0, D3D11)";
        assert_eq!(with_device_id(unknown), unknown);
        assert_eq!(with_device_id("Apple M1"), "Apple M1");
        assert_eq!(with_device_id(""), "");
    }

    #[test]
    fn normalisation_matches_the_generator() {
        assert_eq!(normalize_gpu_name("NVIDIA GeForce RTX 3060"), "geforcertx3060");
        assert_eq!(normalize_gpu_name("Intel(R) UHD Graphics 630"), "uhdgraphics630");
        assert_eq!(normalize_gpu_name("AMD Radeon RX 580 Series"), "radeonrx580");
    }
}

#[cfg(test)]
mod host_identity_tests {
    use super::*;

    #[test]
    fn missing_user_agent_gets_a_host_default() {
        let info = with_host_identity(serde_json::Map::new());
        let ua = info.get("userAgent").and_then(Value::as_str).unwrap();
        assert!(ua.contains("Chrome/158.0.0.0"));
        assert_eq!(info.get("system").and_then(Value::as_str), Some(host_system()));
    }

    #[test]
    fn declared_system_decides_the_platform_and_own_ua_is_kept() {
        let mut map = serde_json::Map::new();
        map.insert("system".into(), Value::String("Linux".into()));
        let info = with_host_identity(map);
        assert!(info.get("userAgent").and_then(Value::as_str).unwrap().contains("(X11; Linux x86_64)"));

        let mut own = serde_json::Map::new();
        own.insert("userAgent".into(), Value::String("custom".into()));
        assert_eq!(with_host_identity(own).get("userAgent").and_then(Value::as_str), Some("custom"));
    }
}
