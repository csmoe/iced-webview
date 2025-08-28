#!/usr/bin/env -S cargo +nightly -Zscript
---cargo
[dependencies]
serde = { version = "1", features = ["derive"] }
plist = "*"
---

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct InfoPlist {
    #[serde(rename = "CFBundleDevelopmentRegion")]
    cf_bundle_development_region: String,
    #[serde(rename = "CFBundleDisplayName")]
    cf_bundle_display_name: String,
    #[serde(rename = "CFBundleExecutable")]
    cf_bundle_executable: String,
    #[serde(rename = "CFBundleIdentifier")]
    cf_bundle_identifier: String,
    #[serde(rename = "CFBundleInfoDictionaryVersion")]
    cf_bundle_info_dictionary_version: String,
    #[serde(rename = "CFBundleName")]
    cf_bundle_name: String,
    #[serde(rename = "CFBundlePackageType")]
    cf_bundle_package_type: String,
    #[serde(rename = "CFBundleSignature")]
    cf_bundle_signature: String,
    #[serde(rename = "CFBundleVersion")]
    cf_bundle_version: String,
    #[serde(rename = "CFBundleShortVersionString")]
    cf_bundle_short_version_string: String,
    #[serde(rename = "LSEnvironment")]
    ls_environment: HashMap<String, String>,
    #[serde(rename = "LSFileQuarantineEnabled")]
    ls_file_quarantine_enabled: bool,
    #[serde(rename = "LSMinimumSystemVersion")]
    ls_minimum_system_version: String,
    #[serde(rename = "LSUIElement")]
    ls_ui_element: String,
    #[serde(rename = "NSSupportsAutomaticGraphicsSwitching")]
    ns_supports_automatic_graphics_switching: bool,
}

const EXEC_PATH: &str = "Contents/MacOS";
const FRAMEWORKS_PATH: &str = "Contents/Frameworks";
const RESOURCES_PATH: &str = "Contents/Resources";
const FRAMEWORK: &str = "Chromium Embedded Framework.framework";
const HELPERS: &[(&str, &str)] = &[
    ("gpu", "webview Helper (GPU)"),
    ("render", "webview Helper (Renderer)"),
    ("plugin", "webview Helper (Plugin)"),
    ("alerts", "webview Helper (Alerts)"),
    ("", "webview Helper"),
];

// - webview.app
//   - Contents
//     - MacOS
//     - Resources
//     - Frameworks
fn create_app_layout(app_path: &Path, is_helper: bool) -> PathBuf {
    for p in [EXEC_PATH, RESOURCES_PATH, FRAMEWORKS_PATH] {
        if is_helper && p == FRAMEWORKS_PATH {
            continue;
        }

        fs::create_dir_all(app_path.join(p)).unwrap()
    }
    app_path.join("Contents")
}

// - webview.app
//   - Contents
//     - Info.plist
//     - MacOS
//       - webview
//     - Resources
//     - Frameworks
fn create_app(app_path: &Path, exec_name: &str, bin: &Path, helper_kind: Option<&str>) -> PathBuf {
    let app_path = app_path.join(exec_name).with_extension("app");
    if app_path.exists() {
        std::fs::remove_dir_all(&app_path).unwrap();
    }
    let contents_path = create_app_layout(&app_path, helper_kind.is_some());
    create_info_plist(&contents_path, exec_name, helper_kind).unwrap();
    fs::copy(bin, app_path.join(EXEC_PATH).join(exec_name)).unwrap();
    app_path
}

fn create_info_plist(
    contents_path: &Path,
    exec_name: &str,
    helper_kind: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let info_plist = InfoPlist {
        cf_bundle_development_region: "en".to_string(),
        cf_bundle_display_name: exec_name.to_string(),
        cf_bundle_executable: exec_name.to_string(),
        cf_bundle_identifier: format!(
            "com.demo{}",
            match helper_kind {
                Some(kind) if kind == "" => format!(".helper"),
                Some(kind) => format!(".helper.{kind}"),
                None => String::from(".webview"),
            }
        ),
        cf_bundle_info_dictionary_version: "6.0".to_string(),
        cf_bundle_name: "webview".to_string(),
        cf_bundle_package_type: "APPL".to_string(),
        cf_bundle_signature: "????".to_string(),
        cf_bundle_version: "1.0.0".to_string(),
        cf_bundle_short_version_string: "1.0".to_string(),
        ls_environment: [("MallocNanoZone".to_string(), "0".to_string())]
            .iter()
            .cloned()
            .collect(),
        ls_file_quarantine_enabled: true,
        ls_minimum_system_version: "11.0".to_string(),
        ls_ui_element: if helper_kind.is_some() { "1" } else { "0" }.to_string(),
        ns_supports_automatic_graphics_switching: true,
    };

    plist::to_file_xml(contents_path.join("Info.plist"), &info_plist)?;
    Ok(())
}

// - webview.app
//   - Contents
//     - Info.plist
//     - MacOS
//     - Resources
//     - Frameworks
//       - Chromium Embedded Framework.framework
//       - webview Helper (GPU).app
//       - webview Helper (Renderer).app
//       - webview Helper (Plugin).app
//       - webview Helper (Alerts).app
//       - webview Helper.app
// See https://bitbucket.org/chromiumembedded/cef/wiki/GeneralUsage.md#markdown-header-macos
pub fn bundle() {
    let example_path = PathBuf::from("./target/release/examples");
    let main_app_path = create_app(
        &example_path,
        "webview",
        &example_path.join("webview"),
        None,
    );
    let to = main_app_path.join(FRAMEWORKS_PATH).join(FRAMEWORK);
    if to.exists() {
        fs::remove_dir_all(&to).unwrap();
    }
    copy_directory(
        &PathBuf::from(std::env::var("CEF_PATH").expect("missing CEF_PATH env")).join(FRAMEWORK),
        &to,
    );
    HELPERS.iter().for_each(|(kind, helper)| {
        create_app(
            &main_app_path.join(FRAMEWORKS_PATH),
            helper,
            &example_path.join("webview"),
            Some(kind),
        );
    });
}

fn copy_directory(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &dst_path);
        } else {
            fs::copy(&entry.path(), &dst_path).unwrap();
        }
    }
}

fn main() {
    bundle();
}
