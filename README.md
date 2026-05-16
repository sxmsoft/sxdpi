<div align="center">
  <img src="https://raw.githubusercontent.com/sxmsoft/sxdpi/main/src/logo.png" alt="SxDPI Logo" width="256">

  # SxDPI 0.1
  **Advanced DPI Bypass Desktop Application**

  [![Tauri](https://img.shields.io/badge/Built%20with-Tauri%20v2-orange?style=for-the-badge&logo=tauri)](https://tauri.app/)
  [![Rust](https://img.shields.io/badge/Backend-Rust-red?style=for-the-badge&logo=rust)](https://rust-lang.org/)
  [![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](#license)
</div>

---

SxDPI is a cutting-edge cross-platform application designed to bypass Deep Packet Inspection (DPI) censorship directly from your desktop. Unlike VPNs which tunnel your entire connection, SxDPI applies advanced packet manipulation and TCP fragmentation to evade network filters while maintaining your native connection speed and low latency.

## 🚀 Features

- **TCP Fragmentation:** Splits the initial packets (like TLS ClientHello) into smaller chunks, bypassing SNI-based DPI rules.
- **DNS over HTTPS (DoH):** Uses secure DoH (Cloudflare/Google) to resolve domains, preventing ISP-level DNS poisoning and spoofing.
- **Combined Bypass Modes:** Use TCP Fragmentation, Fake Packets, and Host Manipulation together or individually.
- **Cross-Platform:** Built with Tauri & Rust, supporting Windows, Linux, macOS, Android, and iOS.
- **Modern & Premium UI:** Red/Black dynamic theme with responsive animations.
- **No System Modifications:** Sets and resets system proxies safely, recovering even during panic/crash events.

## 🛠️ Build & Development

### Prerequisites
- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (latest stable)
- Java JDK & Android Studio (For Android builds)

### 💻 Windows Setup (exe/msi)
To build the Windows executable:
```bash
# Install dependencies
npm install

# Build the executable
npx tauri build
```
You can find your build outputs in `src-tauri/target/release/bundle`.

### 📱 Android Setup (apk)
To build the Android application:
```bash
# Initialize Android
npx tauri android init

# Build the APK
npx tauri android build
```

## 💖 Support the Project

SxDPI is an open-source project created and maintained for free internet access. If you like the project, consider supporting development!

[![Ko-Fi](https://img.shields.io/badge/Ko--fi-F16061?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/ensxm)

## 📄 License
This project is licensed under the MIT License.
