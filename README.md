# Webview Widget for iced-rs

It's in early development stage, almost a demo to verify the chromium embedded framework can be embedded into iced.

<img src="https://github.com/csmoe/iced-webview/blob/main/assets/iced-webview.png?raw=true">

## Development

1. Download the prebuilt cef binaries

```
cargo install export-cef-dir

# linux/macos
export CEF_PATH=$(pwd)/target/cef

# windows powershell
$env:CEF_PATH="$PWD/target/cef"

export-cef-dir --force $CEF_PATH
```

2. Build

```
cargo build --example webview --release

# windows gpu rendering
$env:WGPU_BACKEND="dx12";
cargo build --example webview --release --features hw-renderer

# We need to bundle the binary as application on macOS
./examples/mac_bundler.rs
```

3. Run the example

```
# linux
./target/release/examples/webview

# windows
cp example/webview.exe.manifest target/release/examples/webview.exe.manifest
./target/release/examples/webview.exe

# macOS
open target/release/examples/webview.app
```
