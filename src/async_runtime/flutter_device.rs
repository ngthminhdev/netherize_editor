use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlutterDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub emulator: bool,
    pub is_active: bool,
}

fn resolve_sdk_dir() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("ANDROID_HOME") {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(path) = std::env::var("ANDROID_SDK_ROOT") {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::Path::new(&home)
            .join("Library")
            .join("Android")
            .join("sdk");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn resolve_adb_path() -> String {
    if let Some(sdk_dir) = resolve_sdk_dir() {
        let p = sdk_dir.join("platform-tools").join("adb");
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    for p in &[
        "/opt/homebrew/bin/adb",
        "/usr/local/bin/adb",
        "/usr/bin/adb",
    ] {
        if std::path::Path::new(p).exists() {
            return p.to_string().to_string();
        }
    }
    "adb".to_string()
}

fn resolve_emulator_path() -> String {
    if let Some(sdk_dir) = resolve_sdk_dir() {
        let p = sdk_dir.join("emulator").join("emulator");
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    for p in &[
        "/opt/homebrew/bin/emulator",
        "/usr/local/bin/emulator",
        "/usr/bin/emulator",
    ] {
        if std::path::Path::new(p).exists() {
            return p.to_string().to_string();
        }
    }
    "emulator".to_string()
}

fn make_flutter_command(flutter_path: &str) -> Command {
    if flutter_path.ends_with("fvm") {
        let mut cmd = Command::new(flutter_path);
        cmd.arg("flutter");
        cmd
    } else {
        Command::new(flutter_path)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RawFlutterDevice {
    name: String,
    id: String,
    target_platform: String,
    emulator: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RawFlutterEmulator {
    id: String,
    name: String,
    platform_type: String,
}

#[derive(Deserialize, Debug)]
struct SimctlOutput {
    devices: std::collections::HashMap<String, Vec<SimctlDevice>>,
}

#[derive(Deserialize, Debug)]
struct SimctlDevice {
    name: String,
    udid: String,
    state: String,
    #[serde(rename = "isAvailable")]
    is_available: serde_json::Value,
}

pub async fn scan_flutter_devices(flutter_path: Option<std::path::PathBuf>) -> Vec<FlutterDevice> {
    eprintln!("[scan_flutter_devices] Starting parallel device scan...");
    let flutter_cmd = flutter_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "flutter".to_string());

    let timeout_duration = std::time::Duration::from_secs(5);

    // 1. Run flutter devices --machine
    let f1_cmd = flutter_cmd.clone();
    let handle_flutter_devices = tokio::spawn(async move {
        let mut list = Vec::new();
        eprintln!("[scan_flutter_devices] Spawning 'flutter devices --machine'...");
        let res = tokio::time::timeout(
            timeout_duration,
            make_flutter_command(&f1_cmd)
                .args(&["devices", "--machine"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await;
        match res {
            Ok(Ok(output)) if output.status.success() => {
                if let Ok(raw_devices) =
                    serde_json::from_slice::<Vec<RawFlutterDevice>>(&output.stdout)
                {
                    eprintln!(
                        "[scan_flutter_devices] 'flutter devices' returned {} devices",
                        raw_devices.len()
                    );
                    for dev in raw_devices {
                        list.push(FlutterDevice {
                            id: dev.id,
                            name: dev.name,
                            platform: dev.target_platform,
                            emulator: dev.emulator,
                            is_active: true,
                        });
                    }
                } else {
                    eprintln!("[scan_flutter_devices] Failed to parse 'flutter devices' JSON");
                }
            }
            Ok(Ok(output)) => {
                eprintln!(
                    "[scan_flutter_devices] 'flutter devices' failed with status: {:?}",
                    output.status
                );
            }
            Ok(Err(e)) => {
                eprintln!(
                    "[scan_flutter_devices] Failed to run 'flutter devices': {:?}",
                    e
                );
            }
            Err(_) => {
                eprintln!("[scan_flutter_devices] 'flutter devices' timed out after 5s");
            }
        }
        list
    });

    // 2. Run flutter emulators --machine
    let f2_cmd = flutter_cmd.clone();
    let handle_flutter_emulators = tokio::spawn(async move {
        let mut list = Vec::new();
        eprintln!("[scan_flutter_devices] Spawning 'flutter emulators --machine'...");
        let res = tokio::time::timeout(
            timeout_duration,
            make_flutter_command(&f2_cmd)
                .args(&["emulators", "--machine"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await;
        match res {
            Ok(Ok(output)) if output.status.success() => {
                if let Ok(raw_emulators) =
                    serde_json::from_slice::<Vec<RawFlutterEmulator>>(&output.stdout)
                {
                    eprintln!(
                        "[scan_flutter_devices] 'flutter emulators' returned {} emulators",
                        raw_emulators.len()
                    );
                    for emu in raw_emulators {
                        list.push(FlutterDevice {
                            id: emu.id,
                            name: emu.name,
                            platform: emu.platform_type,
                            emulator: true,
                            is_active: false,
                        });
                    }
                } else {
                    eprintln!("[scan_flutter_devices] Failed to parse 'flutter emulators' JSON");
                }
            }
            Ok(Ok(output)) => {
                eprintln!(
                    "[scan_flutter_devices] 'flutter emulators' failed with status: {:?}",
                    output.status
                );
            }
            Ok(Err(e)) => {
                eprintln!(
                    "[scan_flutter_devices] Failed to run 'flutter emulators': {:?}",
                    e
                );
            }
            Err(_) => {
                eprintln!("[scan_flutter_devices] 'flutter emulators' timed out after 5s");
            }
        }
        list
    });

    // 3. Native iOS Simulator scan via xcrun simctl
    let handle_simctl = tokio::spawn(async move {
        let mut list = Vec::new();
        eprintln!("[scan_flutter_devices] Spawning 'xcrun simctl list devices --json'...");
        let res = tokio::time::timeout(
            timeout_duration,
            Command::new("xcrun")
                .args(&["simctl", "list", "devices", "--json"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await;
        match res {
            Ok(Ok(output)) if output.status.success() => {
                if let Ok(simctl_data) = serde_json::from_slice::<SimctlOutput>(&output.stdout) {
                    let mut count = 0;
                    for (_runtime, devs) in simctl_data.devices {
                        for dev in devs {
                            let is_avail = match &dev.is_available {
                                serde_json::Value::Bool(b) => *b,
                                serde_json::Value::String(s) => {
                                    s.to_lowercase() == "yes" || s.to_lowercase() == "true"
                                }
                                _ => true,
                            };
                            if is_avail {
                                count += 1;
                                let is_active = dev.state == "Booted";
                                list.push(FlutterDevice {
                                    id: dev.udid,
                                    name: dev.name,
                                    platform: "ios".to_string(),
                                    emulator: true,
                                    is_active,
                                });
                            }
                        }
                    }
                    eprintln!(
                        "[scan_flutter_devices] 'xcrun simctl' returned {} available simulators",
                        count
                    );
                } else {
                    eprintln!("[scan_flutter_devices] Failed to parse 'xcrun simctl' JSON");
                }
            }
            Ok(Ok(output)) => {
                eprintln!(
                    "[scan_flutter_devices] 'xcrun simctl' failed with status: {:?}",
                    output.status
                );
            }
            Ok(Err(e)) => {
                eprintln!(
                    "[scan_flutter_devices] Failed to run 'xcrun simctl': {:?}",
                    e
                );
            }
            Err(_) => {
                eprintln!("[scan_flutter_devices] 'xcrun simctl' timed out after 5s");
            }
        }
        list
    });

    // 4. Native Android AVDs scan via emulator -list-avds
    let emulator_cmd = resolve_emulator_path();
    let adb_cmd = resolve_adb_path();

    let e_cmd = emulator_cmd.clone();
    let handle_emulator = tokio::spawn(async move {
        let mut list = Vec::new();
        eprintln!("[scan_flutter_devices] Spawning '{} -list-avds'...", e_cmd);
        let res = tokio::time::timeout(
            timeout_duration,
            Command::new(&e_cmd)
                .arg("-list-avds")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await;
        match res {
            Ok(Ok(output)) if output.status.success() => {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let mut count = 0;
                for line in stdout_str.lines() {
                    let avd_name = line.trim();
                    if !avd_name.is_empty() {
                        count += 1;
                        list.push(FlutterDevice {
                            id: avd_name.to_string(),
                            name: avd_name.replace('_', " "),
                            platform: "android".to_string(),
                            emulator: true,
                            is_active: false,
                        });
                    }
                }
                eprintln!(
                    "[scan_flutter_devices] Android emulator list returned {} AVDs",
                    count
                );
            }
            Ok(Ok(output)) => {
                eprintln!(
                    "[scan_flutter_devices] Android emulator list failed with status: {:?}",
                    output.status
                );
            }
            Ok(Err(e)) => {
                eprintln!(
                    "[scan_flutter_devices] Failed to run Android emulator list: {:?}",
                    e
                );
            }
            Err(_) => {
                eprintln!("[scan_flutter_devices] Android emulator list timed out after 5s");
            }
        }
        list
    });

    // 5. Native active Android devices scan via adb devices
    let a_cmd = adb_cmd.clone();
    let handle_adb = tokio::spawn(async move {
        let mut list = Vec::new();
        eprintln!("[scan_flutter_devices] Spawning '{} devices'...", a_cmd);
        let res = tokio::time::timeout(
            timeout_duration,
            Command::new(&a_cmd)
                .arg("devices")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output(),
        )
        .await;
        match res {
            Ok(Ok(output)) if output.status.success() => {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let mut count = 0;
                for line in stdout_str.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() == 2 && parts[1] == "device" {
                        let id = parts[0];
                        count += 1;
                        list.push(FlutterDevice {
                            id: id.to_string(),
                            name: id.to_string(),
                            platform: "android".to_string(),
                            emulator: id.starts_with("emulator-"),
                            is_active: true,
                        });
                    }
                }
                eprintln!(
                    "[scan_flutter_devices] adb devices returned {} active devices",
                    count
                );
            }
            Ok(Ok(output)) => {
                eprintln!(
                    "[scan_flutter_devices] adb devices failed with status: {:?}",
                    output.status
                );
            }
            Ok(Err(e)) => {
                eprintln!("[scan_flutter_devices] Failed to run adb devices: {:?}", e);
            }
            Err(_) => {
                eprintln!("[scan_flutter_devices] adb devices timed out after 5s");
            }
        }
        list
    });

    // Wait for all branches to complete
    let flutter_devs = handle_flutter_devices.await.unwrap_or_default();
    let flutter_emus = handle_flutter_emulators.await.unwrap_or_default();
    let simctl_devs = handle_simctl.await.unwrap_or_default();
    let avd_emus = handle_emulator.await.unwrap_or_default();
    let adb_devs = handle_adb.await.unwrap_or_default();

    let mut devices = Vec::new();

    // Populate flutter devices and emulators
    for d in flutter_devs {
        if !devices
            .iter()
            .any(|existing: &FlutterDevice| existing.id == d.id)
        {
            devices.push(d);
        }
    }
    for d in flutter_emus {
        if !devices
            .iter()
            .any(|existing: &FlutterDevice| existing.id == d.id)
        {
            devices.push(d);
        }
    }

    // Populate iOS simulators
    for d in simctl_devs {
        if !devices
            .iter()
            .any(|existing: &FlutterDevice| existing.id == d.id)
        {
            devices.push(d);
        }
    }

    // Populate Android AVDs
    for d in avd_emus {
        if !devices
            .iter()
            .any(|existing: &FlutterDevice| existing.id == d.id)
        {
            devices.push(d);
        }
    }

    // Sync active state from ADB list
    for d in adb_devs {
        let mut found = false;
        for existing in &mut devices {
            if existing.id == d.id
                || (d.id.starts_with("emulator-") && existing.platform == "android")
            {
                existing.is_active = true;
                found = true;
            }
        }
        if !found {
            devices.push(d);
        }
    }

    eprintln!(
        "[scan_flutter_devices] Scan completed. Found {} total devices",
        devices.len()
    );
    devices
}

pub async fn launch_flutter_emulator(
    flutter_path: Option<std::path::PathBuf>,
    emulator_id: &str,
) -> Result<(), String> {
    // 1. iOS Simulator boot via xcrun simctl
    let is_ios = emulator_id.contains('-') && emulator_id.len() >= 36;
    if is_ios {
        eprintln!(
            "[launch_flutter_emulator] Booting iOS Simulator: {}",
            emulator_id
        );
        let status = Command::new("xcrun")
            .args(&["simctl", "boot", emulator_id])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let _ = Command::new("open")
            .args(&["-a", "Simulator"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        return match status {
            Ok(s) if s.success() => {
                eprintln!("[launch_flutter_emulator] iOS Simulator boot succeeded");
                Ok(())
            }
            _ => {
                eprintln!("[launch_flutter_emulator] iOS Simulator boot failed");
                Err("Failed to boot iOS simulator".to_string())
            }
        };
    }

    // 2. Android Emulator boot
    let flutter_cmd = flutter_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "flutter".to_string());

    let status = make_flutter_command(&flutter_cmd)
        .args(&["emulators", "--launch", emulator_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match status {
        Ok(s) if s.success() => {
            eprintln!("[launch_flutter_emulator] Flutter emulator launch succeeded");
            Ok(())
        }
        _ => {
            // Fallback: spawn emulator -avd <emulator_id> directly as background process
            let mut emulator_cmd = "emulator".to_string();
            if let Ok(home) = std::env::var("HOME") {
                let sdk_emulator = std::path::Path::new(&home)
                    .join("Library")
                    .join("Android")
                    .join("sdk")
                    .join("emulator")
                    .join("emulator");
                if sdk_emulator.exists() {
                    emulator_cmd = sdk_emulator.to_string_lossy().to_string();
                }
            }

            eprintln!(
                "[launch_flutter_emulator] Falling back to native: {} -avd {}",
                emulator_cmd, emulator_id
            );
            let spawn_result = Command::new(&emulator_cmd)
                .args(&["-avd", emulator_id])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            match spawn_result {
                Ok(_) => {
                    eprintln!("[launch_flutter_emulator] Fallback emulator spawn succeeded");
                    Ok(())
                }
                Err(e) => {
                    eprintln!(
                        "[launch_flutter_emulator] Fallback emulator spawn failed: {:?}",
                        e
                    );
                    Err(format!(
                        "Both flutter emulator launch and fallback emulator launch failed. Fallback error: {}",
                        e
                    ))
                }
            }
        }
    }
}
