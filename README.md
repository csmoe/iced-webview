# Webview Widget for iced-rs

It's in early development stage, almost a demo to verify the chromium embedded framework can be embedded into iced.

<img src="https://github.com/csmoe/iced-webview/blob/main/assets/iced-webview.png?raw=true">

## Development

checkout [cef-rs](https://github.com/tauri-apps/cef-rs) to install needed toolchains.

### Windows

```sh
# powershell

$env:CEF_PATH="target/"
$env:PATH += $env:CEF_PATH;
cargo run --example webview
```

### MacOS

### Linux
