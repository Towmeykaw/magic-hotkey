use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub mod commands;

pub const SERVICE_NAME: &str = "magic-hotkey";

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub name: String,
    pub steps: Vec<PipelineStep>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommandsConfig {
    commands: Vec<CommandDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hotkey: String,
    #[serde(default)]
    pub launch_on_startup: bool,
    #[serde(default)]
    pub auto_paste: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+H".to_string(),
            launch_on_startup: false,
            auto_paste: false,
        }
    }
}

struct HotkeyState {
    main_hotkey: Option<Shortcut>,
    command_shortcuts: Vec<(Shortcut, String)>, // (shortcut, command_name)
}

// Flag to skip clipboard polling after we write to clipboard ourselves
static SKIP_CLIPBOARD_CHANGE: AtomicBool = AtomicBool::new(false);

// ── Config paths ────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("magic-hotkey");
    fs::create_dir_all(&dir).ok();
    dir
}

fn config_path() -> PathBuf {
    config_dir().join("commands.json")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

// ── Settings persistence ────────────────────────────────────────────

fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&contents) {
                return settings;
            }
        }
    }
    let defaults = AppSettings::default();
    save_settings_to_disk(&defaults).ok();
    defaults
}

fn save_settings_to_disk(settings: &AppSettings) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(settings_path(), json).map_err(|e| format!("Write error: {}", e))
}

// ── Commands persistence ────────────────────────────────────────────

fn step(action: &str) -> PipelineStep {
    PipelineStep { action: action.into(), key: None, template: None }
}

fn cmd(name: &str, steps: Vec<PipelineStep>) -> CommandDef {
    CommandDef { name: name.into(), steps, pinned: false, hotkey: None, trigger: None }
}

fn default_commands() -> Vec<CommandDef> {
    vec![
        cmd("Generate GUID", vec![step("generate_guid")]),
        cmd("Timestamp (ISO)", vec![step("timestamp_iso")]),
        cmd("Timestamp (Unix)", vec![step("timestamp_unix")]),
        cmd("Timestamp (UTC)", vec![step("timestamp_utc")]),
        cmd("Unix → Date", vec![step("unix_to_date")]),
        cmd("Date → Unix", vec![step("date_to_unix")]),
        cmd("Format JSON", vec![step("format_json")]),
        cmd("Format XML", vec![step("format_xml")]),
        cmd("Format YAML", vec![step("format_yaml")]),
        cmd("Base64 Encode", vec![step("base64_encode")]),
        cmd("Base64 Decode", vec![step("base64_decode")]),
        cmd("URL Encode", vec![step("url_encode")]),
        cmd("URL Decode", vec![step("url_decode")]),
        cmd("JWT Decode", vec![step("jwt_decode")]),
        cmd("Hex Encode", vec![step("hex_encode")]),
        cmd("Hex Decode", vec![step("hex_decode")]),
        cmd("HTML Decode", vec![step("html_decode")]),
        cmd("Hash MD5", vec![step("hash_md5")]),
        cmd("Hash SHA1", vec![step("hash_sha1")]),
        cmd("Hash SHA256", vec![step("hash_sha256")]),
        cmd("Markdown → HTML", vec![step("md_to_html")]),
        cmd("HTML → Markdown", vec![step("html_to_md")]),
        cmd("Number Convert", vec![step("number_convert")]),
        cmd("Color Convert", vec![step("color_convert")]),
        cmd("JSON → YAML", vec![step("json_to_yaml")]),
        cmd("JSON → TOML", vec![step("json_to_toml")]),
        cmd("YAML → JSON", vec![step("yaml_to_json")]),
        cmd("TOML → JSON", vec![step("toml_to_json")]),
        cmd("Lorem Ipsum (50 words)", vec![PipelineStep { action: "lorem_ipsum".into(), key: Some("50 words".into()), template: None }]),
        cmd("Lorem Ipsum (3 paragraphs)", vec![PipelineStep { action: "lorem_ipsum".into(), key: Some("3 paragraphs".into()), template: None }]),
        cmd("Roll Dice", vec![step("roll")]),
        cmd("Count", vec![step("count")]),
    ]
}

pub fn load_commands() -> Vec<CommandDef> {
    let path = config_path();
    let defaults = default_commands();

    if path.exists() {
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut config) = serde_json::from_str::<CommandsConfig>(&contents) {
                // Merge: add any new default commands that don't exist yet
                // Match by first step action for single-step defaults
                let mut added = false;
                for def in &defaults {
                    let exists = config.commands.iter().any(|c| c.name == def.name);
                    if !exists {
                        config.commands.push(def.clone());
                        added = true;
                    }
                }
                if added {
                    if let Ok(json) = serde_json::to_string_pretty(&config) {
                        fs::write(&path, json).ok();
                    }
                }
                return config.commands;
            }
        }
    }

    let config = CommandsConfig { commands: defaults.clone() };
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        fs::write(&path, json).ok();
    }
    defaults
}

// ── Pipeline execution ──────────────────────────────────────────────

pub fn is_generator(action: &str) -> bool {
    matches!(action, "generate_guid" | "secret" | "timestamp_iso" | "timestamp_unix" | "timestamp_utc" | "snippet" | "lorem_ipsum" | "roll")
}

pub fn run_action(action: &str, input: &str, key: Option<&str>) -> Result<String, String> {
    match action {
        "generate_guid" => commands::generate_guid(),
        "timestamp_iso" => commands::timestamp_iso(),
        "timestamp_unix" => commands::timestamp_unix(),
        "timestamp_utc" => commands::timestamp_utc(),
        "unix_to_date" => commands::unix_to_date(input),
        "date_to_unix" => commands::date_to_unix(input),
        "secret" => {
            let k = key.ok_or("Missing key for secret action")?;
            commands::get_secret(k)
        }
        "lorem_ipsum" => {
            // key holds the count spec (e.g. "5 words", "3 paragraphs")
            let spec = key.unwrap_or("50 words");
            commands::lorem_ipsum(spec)
        }
        "roll" => {
            let spec = key.ok_or("Missing dice notation for roll action (e.g. 1d20, 3d6+2)")?;
            commands::roll_dice(spec)
        }
        "regex_extract" => {
            let pattern = key.ok_or("Missing regex pattern")?;
            commands::regex_extract(input, pattern)
        }
        "format_json" => commands::format_json(input),
        "base64_encode" => commands::base64_encode(input),
        "base64_decode" => commands::base64_decode(input),
        "url_encode" => commands::url_encode(input),
        "url_decode" => commands::url_decode(input),
        "jwt_decode" => commands::jwt_decode(input),
        "hex_encode" => commands::hex_encode(input),
        "hex_decode" => commands::hex_decode(input),
        "html_decode" => commands::html_decode(input),
        "hash_md5" => commands::hash_md5(input),
        "hash_sha1" => commands::hash_sha1(input),
        "hash_sha256" => commands::hash_sha256(input),
        "count" => commands::count(input),
        "format_xml" => commands::format_xml(input),
        "format_yaml" => commands::format_yaml(input),
        "md_to_html" => commands::markdown_to_html(input),
        "html_to_md" => commands::html_to_markdown(input),
        "number_convert" => commands::number_convert(input),
        "color_convert" => commands::color_convert(input),
        "json_to_yaml" => commands::json_to_yaml(input),
        "json_to_toml" => commands::json_to_toml(input),
        "yaml_to_json" => commands::yaml_to_json(input),
        "toml_to_json" => commands::toml_to_json(input),
        "uppercase" => Ok(input.to_uppercase()),
        "lowercase" => Ok(input.to_lowercase()),
        "trim" => Ok(input.trim().to_string()),
        other => Err(format!("Unknown action: {}", other)),
    }
}

// ── Hotkey parsing ──────────────────────────────────────────────────

fn parse_hotkey(hotkey_str: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = hotkey_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return Err("Empty hotkey".to_string());
    }

    let mut modifiers = Modifiers::empty();
    let key_part = parts.last().ok_or("No key specified")?;

    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" => modifiers |= Modifiers::ALT,
            "super" | "win" | "cmd" | "meta" => modifiers |= Modifiers::META,
            _ => return Err(format!("Unknown modifier: {}", part)),
        }
    }

    let code = match key_part.to_lowercase().as_str() {
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC, "d" => Code::KeyD,
        "e" => Code::KeyE, "f" => Code::KeyF, "g" => Code::KeyG, "h" => Code::KeyH,
        "i" => Code::KeyI, "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO, "p" => Code::KeyP,
        "q" => Code::KeyQ, "r" => Code::KeyR, "s" => Code::KeyS, "t" => Code::KeyT,
        "u" => Code::KeyU, "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3,
        "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7,
        "8" => Code::Digit8, "9" => Code::Digit9,
        "space" => Code::Space, "enter" | "return" => Code::Enter, "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape, "backspace" => Code::Backspace,
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3, "f4" => Code::F4,
        "f5" => Code::F5, "f6" => Code::F6, "f7" => Code::F7, "f8" => Code::F8,
        "f9" => Code::F9, "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        "`" | "backquote" => Code::Backquote, "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal, "[" | "bracketleft" => Code::BracketLeft,
        "]" | "bracketright" => Code::BracketRight, "\\" | "backslash" => Code::Backslash,
        ";" | "semicolon" => Code::Semicolon, "'" | "quote" => Code::Quote,
        "," | "comma" => Code::Comma, "." | "period" => Code::Period,
        "/" | "slash" => Code::Slash,
        other => return Err(format!("Unknown key: {}", other)),
    };

    let mods = if modifiers.is_empty() { None } else { Some(modifiers) };
    Ok(Shortcut::new(mods, code))
}

fn register_hotkey(app: &AppHandle, hotkey_str: &str) -> Result<(), String> {
    let shortcut = parse_hotkey(hotkey_str)?;
    let state = app.state::<Mutex<HotkeyState>>();
    let mut hotkey_state = state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = hotkey_state.main_hotkey.take() {
        app.global_shortcut().unregister(old).ok();
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("Failed to register hotkey: {}", e))?;
    hotkey_state.main_hotkey = Some(shortcut);
    Ok(())
}

fn register_command_hotkeys(app: &AppHandle) {
    let state = app.state::<Mutex<HotkeyState>>();
    let mut hotkey_state = state.lock().unwrap_or_else(|e| e.into_inner());

    // Unregister old command shortcuts
    for (old, _) in hotkey_state.command_shortcuts.drain(..) {
        app.global_shortcut().unregister(old).ok();
    }

    // Register new ones from commands
    let cmds = load_commands();
    for cmd in &cmds {
        if let Some(ref hk) = cmd.hotkey {
            if let Ok(shortcut) = parse_hotkey(hk) {
                // Don't register if it conflicts with the main hotkey
                if hotkey_state.main_hotkey.as_ref() != Some(&shortcut) {
                    if app.global_shortcut().register(shortcut).is_ok() {
                        hotkey_state.command_shortcuts.push((shortcut, cmd.name.clone()));
                    }
                }
            }
        }
    }
}

fn maybe_auto_paste() {
    let settings = load_settings();
    if !settings.auto_paste {
        return;
    }
    // Spawn a thread with a delay — give the window time to hide
    // and focus to return to the previous app before simulating paste
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if let Ok(mut enigo) = enigo::Enigo::new(&enigo::Settings::default()) {
            use enigo::{Direction, Key, Keyboard};
            #[cfg(target_os = "macos")]
            let modifier = Key::Meta;
            #[cfg(not(target_os = "macos"))]
            let modifier = Key::Control;

            enigo.key(modifier, Direction::Press).ok();
            enigo.key(Key::Unicode('v'), Direction::Click).ok();
            enigo.key(modifier, Direction::Release).ok();
        }
    });
}

fn execute_pipeline(app: &AppHandle, steps: &[PipelineStep]) -> Result<String, String> {
    if steps.is_empty() {
        return Err("No steps to execute".to_string());
    }

    let first = &steps[0];
    let mut value = if is_generator(&first.action) {
        run_action(&first.action, "", first.key.as_deref())?
    } else {
        let clipboard_text = app
            .clipboard()
            .read_text()
            .map_err(|e| format!("Failed to read clipboard: {}", e))?;
        run_action(&first.action, &clipboard_text, first.key.as_deref())?
    };

    for s in &steps[1..] {
        value = run_action(&s.action, &value, s.key.as_deref())?;
    }

    SKIP_CLIPBOARD_CHANGE.store(true, Ordering::Relaxed);
    app.clipboard()
        .write_text(&value)
        .map_err(|e| format!("Failed to write to clipboard: {}", e))?;

    maybe_auto_paste();
    Ok(value)
}

// ── Tauri commands ──────────────────────────────────────────────────

#[tauri::command]
fn get_commands() -> Vec<CommandDef> {
    load_commands()
}

#[tauri::command]
fn save_commands(app: AppHandle, cmds: Vec<CommandDef>) -> Result<(), String> {
    let config = CommandsConfig { commands: cmds };
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(config_path(), json).map_err(|e| format!("Write error: {}", e))?;
    register_command_hotkeys(&app);
    Ok(())
}

#[tauri::command]
fn execute_command(app: AppHandle, steps: Vec<PipelineStep>) -> Result<String, String> {
    execute_pipeline(&app, &steps)
}

#[tauri::command]
fn execute_with_input(app: AppHandle, input: String, steps: Vec<PipelineStep>) -> Result<String, String> {
    let mut value = input;
    for s in &steps {
        value = run_action(&s.action, &value, s.key.as_deref())?;
    }
    SKIP_CLIPBOARD_CHANGE.store(true, Ordering::Relaxed);
    app.clipboard()
        .write_text(&value)
        .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
    maybe_auto_paste();
    Ok(value)
}

#[tauri::command]
fn detect_clipboard(app: AppHandle) -> Vec<String> {
    app.clipboard()
        .read_text()
        .map(|text| commands::detect_content(&text))
        .unwrap_or_default()
}

#[tauri::command]
fn toggle_pin(name: String) -> Result<(), String> {
    let mut cmds = load_commands();
    if let Some(cmd) = cmds.iter_mut().find(|c| c.name == name) {
        cmd.pinned = !cmd.pinned;
    }
    let config = CommandsConfig { commands: cmds };
    let json = serde_json::to_string_pretty(&config).map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(config_path(), json).map_err(|e| format!("Write error: {}", e))
}

#[tauri::command]
fn store_secret(key: String, value: String) -> Result<(), String> {
    commands::set_secret(&key, &value)
}

#[tauri::command]
fn delete_secret(key: String) -> Result<(), String> {
    commands::delete_secret(&key)
}

#[tauri::command]
fn get_settings() -> AppSettings {
    load_settings()
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    register_hotkey(&app, &settings.hotkey)?;
    let autostart = app.autolaunch();
    if settings.launch_on_startup {
        autostart.enable().map_err(|e| format!("Failed to enable autostart: {}", e))?;
    } else {
        autostart.disable().map_err(|e| format!("Failed to disable autostart: {}", e))?;
    }
    save_settings_to_disk(&settings)
}

#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().ok();
    }
}

#[tauri::command]
fn get_config_path() -> String {
    config_dir().to_string_lossy().to_string()
}

// ── Window toggle ───────────────────────────────────────────────────

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            window.hide().ok();
        } else {
            window.center().ok();
            window.show().ok();
            window.set_focus().ok();
            app.emit("focus-search", ()).ok();
        }
    }
}

// ── App entry ───────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = load_settings();
    let initial_hotkey = settings.hotkey.clone();

    tauri::Builder::default()
        .manage(Mutex::new(HotkeyState { main_hotkey: None, command_shortcuts: Vec::new() }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(move |app| {
            let main_shortcut = parse_hotkey(&initial_hotkey)
                .unwrap_or_else(|_| Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyH));

            let handle = app.handle().clone();
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        if event.state != ShortcutState::Pressed {
                            return;
                        }
                        let state = handle.state::<Mutex<HotkeyState>>();
                        let hotkey_state = state.lock().unwrap_or_else(|e| e.into_inner());

                        // Check if it's the main hotkey
                        if hotkey_state.main_hotkey.as_ref() == Some(shortcut) {
                            drop(hotkey_state);
                            toggle_window(&handle);
                            return;
                        }

                        // Check if it's a command hotkey
                        if let Some((_, cmd_name)) = hotkey_state.command_shortcuts.iter().find(|(s, _)| s == shortcut) {
                            let name = cmd_name.clone();
                            drop(hotkey_state);
                            // Emit event to frontend to execute this command
                            handle.emit("execute-by-name", &name).ok();
                            // Show window so the frontend can show toast/snippet/overlay
                            if let Some(w) = handle.get_webview_window("main") {
                                w.center().ok();
                                w.show().ok();
                                w.set_focus().ok();
                            }
                        }
                    })
                    .build(),
            )?;

            app.global_shortcut().register(main_shortcut).expect("Failed to register global hotkey");
            app.state::<Mutex<HotkeyState>>().lock().unwrap().main_hotkey = Some(main_shortcut);

            // Register command-specific hotkeys
            register_command_hotkeys(app.handle());

            // Hide window on focus loss
            if let Some(window) = app.get_webview_window("main") {
                let h = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        if let Some(w) = h.get_webview_window("main") {
                            w.hide().ok();
                        }
                    }
                });
            }

            // ── Clipboard polling for workflow triggers ──────────────
            let trigger_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_content = String::new();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Skip if we just wrote to clipboard ourselves
                    if SKIP_CLIPBOARD_CHANGE.swap(false, Ordering::Relaxed) {
                        // Update last_content so we don't re-trigger
                        if let Ok(text) = trigger_handle.clipboard().read_text() {
                            last_content = text;
                        }
                        continue;
                    }

                    let current = match trigger_handle.clipboard().read_text() {
                        Ok(text) => text,
                        Err(_) => continue,
                    };

                    if current == last_content || current.trim().is_empty() {
                        continue;
                    }
                    last_content = current.clone();

                    // Check all commands for matching triggers
                    let cmds = load_commands();
                    for cmd in &cmds {
                        if let Some(ref pattern) = cmd.trigger {
                            if let Ok(re) = regex::Regex::new(pattern) {
                                if re.is_match(&current) {
                                    // Skip commands that need runtime prompts (snippets, keyless rolls)
                                    let needs_prompt = cmd.steps.iter().any(|s|
                                        s.action == "snippet"
                                        || (s.action == "roll" && s.key.is_none())
                                    );
                                    if needs_prompt {
                                        continue;
                                    }
                                    // Execute the pipeline
                                    if let Ok(_) = execute_pipeline(&trigger_handle, &cmd.steps) {
                                        // OS notification
                                        trigger_handle.notification()
                                            .builder()
                                            .title("Magic Hotkey")
                                            .body(format!("Auto-ran: {}", cmd.name))
                                            .show()
                                            .ok();
                                    }
                                    break; // Only fire first matching trigger
                                }
                            }
                        }
                    }
                }
            });

            // ── System tray ─────────────────────────────────────────
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            let tray_icon = Image::from_bytes(include_bytes!("../../magic-hotkey.png"))
                .expect("failed to load tray icon");

            let tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip("Magic Hotkey")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                w.center().ok();
                                w.show().ok();
                                w.set_focus().ok();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            w.center().ok();
                            w.show().ok();
                            w.set_focus().ok();
                        }
                    }
                })
                .build(app)?;

            // Store the tray icon in managed state so it isn't dropped
            app.manage(Mutex::new(tray));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_commands, save_commands, execute_command, execute_with_input,
            detect_clipboard, toggle_pin,
            store_secret, delete_secret,
            get_settings, save_settings,
            hide_window, get_config_path, get_autostart_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
