<div align="center">
  <img src="https://raw.githubusercontent.com/sxmsoft/sxdpi/main/src/logo.png" alt="SxDPI Logo" width="256">

  # SxDPI 1.2.0
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

## 🛡️ VirusTotal & Security Analysis (False-Positive)

SxDPI is a completely open-source project. It does not track user data, collect logs, or tunnel your traffic to any remote servers. 

The VirusTotal scan result for the `SxDPI_1.2.0_x64-setup.exe` file is **2/71**. All major antivirus engines, including Microsoft Defender, Kaspersky, and BitDefender, have fully verified that the application is completely safe and undetected.

* **VirusTotal Report:** (https://www.virustotal.com/gui/file/4ddb8662105371725b960f9fdae9d1561e7a599191ce8be65a4c1141ebd62043/detection)

### Why do 2 antivirus engines flag it as "Unsafe / Malicious"?

This is a textbook example of a **False-Positive** detection. Due to the core mechanics of how SxDPI operates, certain highly sensitive or lesser-known security vendors (specifically Arctic Wolf and SecureAge) flag the binary based on the following behaviors:

1.  **DPI Bypass Mechanism:** To bypass ISP-level censorship, the app splits outgoing TCP packets locally (TCP fragmentation). This deep-level network manipulation can easily trigger heuristic network monitoring flags in some antivirus software.
2.  **System Network Automation:** When SxDPI starts, it automatically configures Windows proxy settings. Upon closing, it cleans up the Registry and flushes network settings to ensure your internet connection doesn't drop. Any independent `.exe` that alters system-level network configurations is often flagged as suspicious by default.
3.  **Code Signing Certificate:** Since this is a community-driven, open-source project built by an independent developer, the binary is not signed with expensive corporate Microsoft EV Certificates (which cost thousands of dollars annually). Some engines automatically label unsigned executables as untrusted.

> 💡 **Note:** If you have any security concerns, you are more than welcome to audit the entire source code, inspect the dependencies, and follow the `📦 Installation & Building` guide to compile the application locally from scratch on your own machine.


## 💖 Support the Project

SxDPI is an open-source project created and maintained for free internet access. If you like the project, consider supporting development!

[![Ko-Fi](https://img.shields.io/badge/Ko--fi-F16061?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/ensxm)

## 📄 License
This project is licensed under the MIT License.

