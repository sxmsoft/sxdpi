// ═══════════════════════════════════════════════════════════════
// SxDPI — Frontend Application Logic
// Tauri invoke ile backend iletişimi, state yönetimi, UI kontrol
// ═══════════════════════════════════════════════════════════════

(function () {
    "use strict";

    // ─── Tauri API ──────────────────────────────────────────────

    const invoke = window.__TAURI__?.core?.invoke;
    const listen = window.__TAURI__?.event?.listen;

    // ─── Application State ──────────────────────────────────────

    const state = {
        connected: false,
        connecting: false,
        sidebarOpen: false,
        activePanel: "dashboard",
        settings: {
            bypass_mode: "tcp_fragmentation",
            fragment_size: 2,
            fragment_delay_ms: 50,
            proxy_port: 8118,
            autostart: false,
            ttl_value: 1,
            enable_host_mixcase: true,
            enable_dot_after_host: false,
            enable_header_padding: false,
        },
    };

    // ─── DOM References ─────────────────────────────────────────

    const $ = (sel) => document.querySelector(sel);
    const $$ = (sel) => document.querySelectorAll(sel);

    const dom = {
        hamburgerBtn: $("#hamburger-btn"),
        sidebar: $("#sidebar"),
        sidebarOverlay: $("#sidebar-overlay"),
        connectBtn: $("#connect-btn"),
        connectRing: $("#connect-ring"),
        statusDot: $("#status-dot"),
        statusText: $("#status-text"),
        toast: $("#toast"),
        saveBtn: $("#save-settings-btn"),
        flushBtn: $("#flush-dpi-btn"),
        donateKofi: $("#btn-donate-kofi"),
        // Info cards
        infoMode: $("#info-mode"),
        infoPort: $("#info-port"),
        infoFragment: $("#info-fragment"),
        // Settings inputs
        language: $("#setting-language"),
        bypassMode: $("#setting-bypass-mode"),
        fragmentSize: $("#setting-fragment-size"),
        fragmentDelay: $("#setting-fragment-delay"),
        proxyPort: $("#setting-proxy-port"),
        autostart: $("#setting-autostart"),
        // Range value displays
        fragmentSizeValue: $("#fragment-size-value"),
        fragmentDelayValue: $("#fragment-delay-value"),
    };

    // ─── i18n Translations ──────────────────────────────────────
    const i18n = {
        en: {
            nav_dashboard: "Dashboard",
            nav_settings: "Settings",
            nav_donate: "Donate",
            status_disconnected: "Disconnected",
            status_connecting: "Connecting...",
            status_connected: "Connected",
            info_mode: "Mode",
            info_fragment: "Fragment",
            settings_title: "Settings",
            setting_mode: "DPI Bypass Mode",
            mode_tcp: "TCP Fragmentation",
            mode_fake: "Fake Packet",
            mode_host: "Host Manipulation",
            mode_combined: "Combined (All)",
            setting_frag_size: "Fragment Size (byte)",
            setting_frag_delay: "Fragment Delay (ms)",
            setting_proxy_port: "Proxy Port",
            setting_autostart: "Auto Start",
            setting_save: "Save Settings",
            setting_flush: "Manual Cleanup (Flush DPI)",
            donate_title: "Donate",
            donate_desc: "SxDPI is an open-source and free project. If you want to support development, you can do so via Ko-Fi.",
            donate_kofi: "Support via Ko-Fi!",
            donate_thanks: "Thank you for your support! 🖤"
        },
        tr: {
            nav_dashboard: "Ana Ekran",
            nav_settings: "Ayarlar",
            nav_donate: "Bağış",
            status_disconnected: "Bağlantı Kesildi",
            status_connecting: "Bağlanıyor...",
            status_connected: "Bağlandı",
            info_mode: "Mod",
            info_fragment: "Parça",
            settings_title: "Ayarlar",
            setting_mode: "DPI Bypass Modu",
            mode_tcp: "TCP Fragmentation",
            mode_fake: "Fake Packet",
            mode_host: "Host Manipulation",
            mode_combined: "Combined (Tümü)",
            setting_frag_size: "Parça Boyutu (byte)",
            setting_frag_delay: "Parça Gecikmesi (ms)",
            setting_proxy_port: "Proxy Portu",
            setting_autostart: "Otomatik Başlat",
            setting_save: "Ayarları Kaydet",
            setting_flush: "Manuel Temizlik (Flush DPI)",
            donate_title: "Bağış",
            donate_desc: "SxDPI açık kaynaklı ve ücretsiz bir projedir. Geliştirmeye devam etmemize destek olmak isterseniz Ko-Fi üzerinden destek olabilirsiniz.",
            donate_kofi: "Ko-Fi üzerinden destek ol!",
            donate_thanks: "Desteğiniz için teşekkürler! 🖤"
        }
    };

    function applyLanguage(lang) {
        const dict = i18n[lang] || i18n.en;
        $$('[data-i18n]').forEach(el => {
            const key = el.getAttribute('data-i18n');
            if (dict[key]) {
                el.textContent = dict[key];
            }
        });
        
        // Update status text separately since it changes dynamically
        updateUI(state.connected ? "connected" : state.connecting ? "connecting" : "disconnected");
    }

    // ─── Mode Display Names ─────────────────────────────────────

    const modeNames = {
        tcp_fragmentation: "TCP Frag",
        fake_packet: "Fake Packet",
        host_manipulation: "Host Manip",
        combined: "Combined",
    };

    // ─── Initialize ─────────────────────────────────────────────

    function init() {
        bindEvents();
        loadSettings();
        listenBackendEvents();
        updateInfoCards();
    }

    // ─── Event Bindings ─────────────────────────────────────────

    function bindEvents() {
        // Hamburger menü
        dom.hamburgerBtn.addEventListener("click", toggleSidebar);
        dom.sidebarOverlay.addEventListener("click", closeSidebar);

        // Navigation
        $$(".nav-item").forEach((item) => {
            item.addEventListener("click", () => {
                const panel = item.dataset.panel;
                switchPanel(panel);
                closeSidebar();
            });
        });

        // Connect butonu
        dom.connectBtn.addEventListener("click", handleConnect);

        // Range slider güncellemeleri
        dom.fragmentSize.addEventListener("input", (e) => {
            dom.fragmentSizeValue.textContent = e.target.value;
        });
        dom.fragmentDelay.addEventListener("input", (e) => {
            dom.fragmentDelayValue.textContent = e.target.value;
        });

        // Language change live preview
        dom.language.addEventListener("change", (e) => {
            applyLanguage(e.target.value);
        });

        // Ayarları kaydet
        dom.saveBtn.addEventListener("click", saveSettings);

        // Manuel Temizlik
        if (dom.flushBtn) {
            dom.flushBtn.addEventListener("click", async () => {
                if (invoke) {
                    try {
                        const msg = await invoke("flush_dpi");
                        showToast(msg, "success");
                    } catch (e) {
                        showToast("Hata: " + e, "error");
                    }
                } else {
                    showToast("Flush DPI sadece masaüstünde çalışır.", "error");
                }
            });
        }

        // Bağış linki (Güvenli şekilde tarayıcıda açar)
        if (dom.donateKofi) {
            dom.donateKofi.addEventListener("click", () => {
                if (invoke) {
                    invoke("plugin:shell|open", { path: "https://ko-fi.com/ensxm" });
                } else {
                    window.open("https://ko-fi.com/ensxm", "_blank");
                }
            });
        }

        // Keyboard shortcuts
        document.addEventListener("keydown", (e) => {
            if (e.key === "Escape" && state.sidebarOpen) {
                closeSidebar();
            }
        });

        // DevTools/Sağ Tık İncele engelleme
        document.addEventListener("contextmenu", (e) => {
            if (window.location.hostname !== "localhost") {
                e.preventDefault();
            } else {
                // Dev modunda izin verilmesin istiyorsa direkt kapat
                e.preventDefault();
            }
        });
    }

    // ─── Sidebar ────────────────────────────────────────────────

    function toggleSidebar() {
        state.sidebarOpen ? closeSidebar() : openSidebar();
    }

    function openSidebar() {
        state.sidebarOpen = true;
        dom.sidebar.classList.add("open");
        dom.sidebarOverlay.classList.add("visible");
        dom.hamburgerBtn.classList.add("active");
    }

    function closeSidebar() {
        state.sidebarOpen = false;
        dom.sidebar.classList.remove("open");
        dom.sidebarOverlay.classList.remove("visible");
        dom.hamburgerBtn.classList.remove("active");
    }

    // ─── Panel Switching ────────────────────────────────────────

    function switchPanel(panelName) {
        state.activePanel = panelName;

        // Panelleri güncelle
        $$(".panel").forEach((p) => p.classList.remove("active"));
        const target = $(`#panel-${panelName}`);
        if (target) target.classList.add("active");

        // Nav öğelerini güncelle
        $$(".nav-item").forEach((item) => {
            item.classList.toggle("active", item.dataset.panel === panelName);
        });
    }

    // ─── Connect / Disconnect ───────────────────────────────────

    async function handleConnect() {
        if (state.connecting) return;

        if (state.connected) {
            await disconnect();
        } else {
            await connect();
        }
    }

    async function connect() {
        state.connecting = true;
        updateUI("connecting");

        try {
            if (invoke) {
                const result = await invoke("connect_dpi");
                console.log("Connect:", result);
            }
            state.connected = true;
            state.connecting = false;
            updateUI("connected");
            showToast("DPI Bypass aktif!", "success");
        } catch (err) {
            state.connecting = false;
            state.connected = false;
            updateUI("disconnected");
            showToast(`Bağlantı hatası: ${err}`, "error");
            console.error("Connect error:", err);
        }
    }

    async function disconnect() {
        state.connecting = true;
        updateUI("connecting");

        try {
            if (invoke) {
                const result = await invoke("disconnect_dpi");
                console.log("Disconnect:", result);
            }
            state.connected = false;
            state.connecting = false;
            updateUI("disconnected");
            showToast("Bağlantı kesildi", "success");
        } catch (err) {
            state.connecting = false;
            updateUI(state.connected ? "connected" : "disconnected");
            showToast(`Bağlantı kesme hatası: ${err}`, "error");
            console.error("Disconnect error:", err);
        }
    }

    // ─── UI State Updates ───────────────────────────────────────

    function updateUI(status) {
        // Status dot
        dom.statusDot.className = "status-dot " + status;

        // Status text (Localization applied)
        const dict = i18n[dom.language.value] || i18n.en;
        const texts = {
            connected: dict.status_connected,
            disconnected: dict.status_disconnected,
            connecting: dict.status_connecting,
        };
        dom.statusText.textContent = texts[status] || status;

        // Connect button
        dom.connectBtn.className = "connect-btn " + status;

        // Connect ring
        dom.connectRing.className = "connect-ring " + status;
    }

    function updateInfoCards() {
        const s = state.settings;
        dom.infoMode.textContent = modeNames[s.bypass_mode] || s.bypass_mode;
        dom.infoPort.textContent = s.proxy_port;
        dom.infoFragment.textContent = s.fragment_size + " byte";
    }

    // ─── Settings ───────────────────────────────────────────────

    async function loadSettings() {
        try {
            if (invoke) {
                const settings = await invoke("load_settings");
                if (settings) {
                    state.settings = settings;
                    applySettingsToUI(settings);
                    updateInfoCards();
                }
            }
        } catch (err) {
            console.warn("Ayarlar yüklenemedi, varsayılanlar kullanılıyor:", err);
        }
    }

    function applySettingsToUI(s) {
        if (s.language) {
            dom.language.value = s.language;
            applyLanguage(s.language);
        }
        dom.bypassMode.value = s.bypass_mode;
        dom.fragmentSize.value = s.fragment_size;
        dom.fragmentSizeValue.textContent = s.fragment_size;
        dom.fragmentDelay.value = s.fragment_delay_ms;
        dom.fragmentDelayValue.textContent = s.fragment_delay_ms;
        dom.proxyPort.value = s.proxy_port;
        dom.autostart.checked = s.autostart;
    }

    function collectSettingsFromUI() {
        return {
            language: dom.language.value,
            bypass_mode: dom.bypassMode.value,
            fragment_size: parseInt(dom.fragmentSize.value, 10),
            fragment_delay_ms: parseInt(dom.fragmentDelay.value, 10),
            proxy_port: parseInt(dom.proxyPort.value, 10),
            autostart: dom.autostart.checked,
        };
    }

    async function saveSettings() {
        const settings = collectSettingsFromUI();
        state.settings = settings;
        updateInfoCards();

        try {
            if (invoke) {
                await invoke("save_settings", { settings });
            }
            // Saved feedback
            const dict = i18n[settings.language] || i18n.en;
            dom.saveBtn.classList.add("saved");
            showToast(dict.nav_settings + " \u2713", "success");

            setTimeout(() => {
                dom.saveBtn.classList.remove("saved");
            }, 2000);
        } catch (err) {
            showToast(`Ayarlar kaydedilemedi: ${err}`, "error");
            console.error("Save settings error:", err);
        }
    }

    // ─── Backend Event Listener ─────────────────────────────────

    function listenBackendEvents() {
        if (!listen) return;

        listen("engine-state-changed", (event) => {
            const newState = event.payload;
            console.log("Engine state changed:", newState);

            if (newState === "running") {
                state.connected = true;
                state.connecting = false;
                updateUI("connected");
            } else if (newState === "stopped") {
                state.connected = false;
                state.connecting = false;
                updateUI("disconnected");
            }
        });
    }

    // ─── Toast Notifications ────────────────────────────────────

    let toastTimeout = null;

    function showToast(message, type = "success") {
        if (toastTimeout) clearTimeout(toastTimeout);

        dom.toast.textContent = message;
        dom.toast.className = "toast " + type;

        // Force reflow for animation restart
        void dom.toast.offsetWidth;
        dom.toast.classList.add("visible");

        toastTimeout = setTimeout(() => {
            dom.toast.classList.remove("visible");
        }, 3000);
    }

    // ─── Clipboard ──────────────────────────────────────────────

    async function copyToClipboard(text) {
        try {
            await navigator.clipboard.writeText(text);
            showToast("Adres kopyalandı!", "success");
        } catch {
            // Fallback
            const textarea = document.createElement("textarea");
            textarea.value = text;
            textarea.style.position = "fixed";
            textarea.style.opacity = "0";
            document.body.appendChild(textarea);
            textarea.select();
            document.execCommand("copy");
            document.body.removeChild(textarea);
            showToast("Adres kopyalandı!", "success");
        }
    }

    // ─── Start ──────────────────────────────────────────────────

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();
