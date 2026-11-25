// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Lex Studio: a small pop-up dictionary for translating. A key combination (anywhere, also when another app is in
//! front) brings the little window up at the mouse pointer, with the copied word already looked up; Escape hides it
//! again. Closing keeps the app in the background (tray icon) so the key keeps working.
//! The words come from Wiktionary (lookup.rs); answers are kept on disk (cache.rs) and work offline afterwards.

mod appicon;
mod cache;
mod lang;
mod lookup;
mod mcp;
mod mcp_tools;
mod parse;
mod shared;
mod wikitext;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_opener::OpenerExt;

/// How the pop-up behaves (set from the settings in the window).
struct Popup {
    /// Open at the mouse pointer (else where it was).
    at_cursor: AtomicBool,
    /// Closing the window only hides it (the app stays in the tray, the key keeps working).
    background: AtomicBool,
    hotkey: Mutex<String>,
    tray: Mutex<Option<(tauri::menu::MenuItem<tauri::Wry>, tauri::menu::MenuItem<tauri::Wry>)>>,
}

struct Store(Mutex<cache::Cache>);

/// The app's texts in English and German (i18n/*.json, built into the program).
#[tauri::command]
fn texts() -> serde_json::Value {
    let parse = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap_or_default();
    serde_json::json!({"en": parse(include_str!("../i18n/en.json")), "de": parse(include_str!("../i18n/de.json"))})
}

/* ---------- Looking up ---------- */

/// Looks a word up. A recent answer comes from the store; without internet an older one does too (`cached: true`).
#[tauri::command]
async fn lookup(app: AppHandle, word: String, from: String, to: String, fresh: Option<bool>) -> Result<serde_json::Value, String> {
    let key = cache::key(&word, &from, &to);
    let stored = app.state::<Store>().0.lock().map_err(|_| "busy")?.get(&key);
    if let Some((v, age)) = &stored {
        if *age < cache::FRESH_DAYS && !fresh.unwrap_or(false) { return Ok(with_flag(v.clone(), true)); }
    }
    let (w, f, t) = (word.clone(), from.clone(), to.clone());
    let r = tauri::async_runtime::spawn_blocking(move || lookup::lookup(&lookup::Http::new(), &w, &f, &t)).await.map_err(|e| e.to_string())?;
    match r {
        Ok(l) => {
            let v = serde_json::to_value(&l).map_err(|e| e.to_string())?;
            // Only complete answers are kept (a half-failed one would hide the full one for a month).
            if l.found() && !l.partial { app.state::<Store>().0.lock().map_err(|_| "busy")?.put(&key, v.clone()); }
            Ok(with_flag(v, false))
        }
        Err(e) if e == "offline" || e == "rate-limit" => match stored { Some((v, _)) => Ok(with_flag(v, true)), None => Err(e) },
        Err(e) => Err(e),
    }
}

fn with_flag(mut v: serde_json::Value, cached: bool) -> serde_json::Value {
    if let Some(o) = v.as_object_mut() { o.insert("cached".into(), cached.into()); }
    v
}

/// Words starting like `prefix` (while typing).
#[tauri::command]
async fn suggest(prefix: String, from: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || lookup::suggest(&lookup::Http::new(), &prefix, &from)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn cache_info(store: State<Store>) -> usize { store.0.lock().map(|c| c.len()).unwrap_or(0) }

#[tauri::command]
fn cache_clear(store: State<Store>) { if let Ok(mut c) = store.0.lock() { c.clear(); } }

/* ---------- The pop-up ---------- */

/// Brings the window up (at the mouse pointer when wanted) and tells the page — or hides it when the key is pressed
/// while it is already in front.
fn summon(app: &AppHandle, toggle: bool) {
    let Some(w) = app.get_webview_window("main") else { return };
    if toggle && w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    if app.state::<Popup>().at_cursor.load(Ordering::Relaxed) && !w.is_visible().unwrap_or(false) { place_at_cursor(app, &w); }
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit("summoned", ());
}

fn place_at_cursor(app: &AppHandle, w: &tauri::WebviewWindow) {
    let Ok(c) = app.cursor_position() else { return };
    let Ok(Some(m)) = app.monitor_from_point(c.x, c.y) else { return };
    let size = w.outer_size().unwrap_or_default();
    let area = m.work_area();
    let (ax, ay) = (area.position.x as f64, area.position.y as f64);
    let (aw, ah) = (area.size.width as f64, area.size.height as f64);
    let gap = 14.0 * m.scale_factor();
    let x = (c.x + gap).min(ax + aw - size.width as f64).max(ax);
    // Below the pointer when there is room, else above it.
    let y = if c.y + gap + size.height as f64 <= ay + ah { c.y + gap } else { (c.y - gap - size.height as f64).max(ay) };
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Takes over the pop-up settings: the key combination (e.g. "CommandOrControl+Alt+Shift+W", "" = none), opening at the
/// pointer, staying in the background. Err when the key is not valid or another program already uses it.
#[tauri::command]
fn popup_config(app: AppHandle, popup: State<Popup>, hotkey: String, at_cursor: bool, background: bool) -> Result<(), String> {
    popup.at_cursor.store(at_cursor, Ordering::Relaxed);
    popup.background.store(background, Ordering::Relaxed);
    let mut cur = popup.hotkey.lock().map_err(|_| "busy")?;
    if *cur == hotkey { return Ok(()); }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    *cur = String::new();
    if hotkey.is_empty() { return Ok(()); }
    let s: Shortcut = hotkey.parse().map_err(|e| format!("key: {e}"))?;
    gs.register(s).map_err(|e| format!("taken: {e}"))?;
    *cur = hotkey;
    Ok(())
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) { let _ = window.hide(); }

#[tauri::command]
fn pin(window: tauri::WebviewWindow, on: bool) -> Result<(), String> { window.set_always_on_top(on).map_err(|e| e.to_string()) }

#[tauri::command]
fn quit(app: AppHandle) { app.exit(0); }

/// The tray menu in the window's language.
#[tauri::command]
fn tray_labels(popup: State<Popup>, show: String, quit: String) {
    if let Ok(g) = popup.tray.lock() {
        if let Some((a, b)) = g.as_ref() { let _ = a.set_text(show); let _ = b.set_text(quit); }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let show = MenuItemBuilder::with_id("tray-show", "Lex Studio").build(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).item(&show).separator().item(&quit).build()?;
    let mut b = TrayIconBuilder::with_id("lex").menu(&menu).tooltip("Lex Studio").show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "tray-show" => summon(app, false),
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e { summon(tray.app_handle(), true); }
        });
    if let Some(i) = app.default_window_icon() { b = b.icon(i.clone()); }
    b.build(app)?;
    *app.state::<Popup>().tray.lock().unwrap() = Some((show, quit));
    Ok(())
}

/* ---------- Files, links, clipboard, menus ---------- */

/// Saves the word list as CSV (for Plan Studio's vocabulary trainer, Anki, Excel …).
#[tauri::command]
async fn export_csv(app: AppHandle, name: String, label: String, content: String) -> Result<Option<String>, String> {
    let Some(f) = shared::file_dialog(&app).set_file_name(format!("{name}.csv")).add_filter(&label, &["csv"]).blocking_save_file() else { return Ok(None) };
    let p = f.into_path().map_err(|e| e.to_string())?;
    // With a BOM so Excel reads the umlauts and Greek letters right.
    std::fs::write(&p, format!("\u{feff}{content}")).map_err(|e| e.to_string())?;
    Ok(Some(p.to_string_lossy().into_owned()))
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) { return Err("not a web address".into()); }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
struct MenuEntry {
    id: Option<String>,
    label: Option<String>,
    enabled: Option<bool>,
    checked: Option<bool>,
    accel: Option<String>,
}

/// Shows a native (OS) context menu at the mouse position; the chosen id reaches the UI as a "context-menu" event.
#[tauri::command]
fn context_menu(app: AppHandle, window: tauri::Window, items: Vec<MenuEntry>) -> Result<(), String> {
    use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
    let mut menu = MenuBuilder::new(&app);
    for entry in items {
        let Some(id) = entry.id else { menu = menu.separator(); continue };
        let label = entry.label.unwrap_or_default();
        let enabled = entry.enabled.unwrap_or(true);
        if let Some(checked) = entry.checked {
            let item = CheckMenuItemBuilder::with_id(id, label).enabled(enabled).checked(checked).build(&app).map_err(|e| e.to_string())?;
            menu = menu.item(&item);
            continue;
        }
        let plain = || MenuItemBuilder::with_id(id.clone(), label.clone()).enabled(enabled);
        let item = match entry.accel.as_deref() {
            Some(a) => plain().accelerator(a).build(&app).or_else(|_| plain().build(&app)),
            None => plain().build(&app),
        }
        .map_err(|e| e.to_string())?;
        menu = menu.item(&item);
    }
    let menu = menu.build().map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

#[tauri::command]
fn clipboard_read(app: AppHandle) -> String {
    app.clipboard().read_text().unwrap_or_default()
}

#[tauri::command]
fn clipboard_write(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

fn main() {
    if mcp::wanted() { return mcp_tools::run(); }
    let hidden = std::env::args().skip(1).any(|a| a == "--hidden");
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, _s, e| {
            if e.state() == ShortcutState::Pressed { summon(app, true); }
        }).build())
        .on_menu_event(|app, event| {
            if !event.id().as_ref().starts_with("tray-") { let _ = app.emit("context-menu", event.id().as_ref()); }
        })
        .manage(Popup { at_cursor: AtomicBool::new(true), background: AtomicBool::new(true), hotkey: Mutex::new(String::new()), tray: Mutex::new(None) })
        .setup(move |app| {
            let path = app.path().app_data_dir().ok().map(|d| d.join("lookups.json"));
            app.manage(Store(Mutex::new(cache::Cache::open(path))));
            if let Err(e) = build_tray(app.handle()) { eprintln!("tray: {e}"); }
            if hidden { if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); } }
            Ok(())
        })
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                if w.app_handle().state::<Popup>().background.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            appicon::set_dock_icon,
            shared::shared_get,
            shared::shared_set,
            texts,
            lookup,
            suggest,
            cache_info,
            cache_clear,
            popup_config,
            hide_window,
            pin,
            quit,
            tray_labels,
            export_csv,
            open_url,
            context_menu,
            clipboard_read,
            clipboard_write
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lex Studio");

    app.run(|_app, _event| {
        // macOS: a click on the Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event { summon(_app, false); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_parse_and_match() {
        let t = texts();
        let (en, de) = (t["en"].as_object().unwrap(), t["de"].as_object().unwrap());
        assert!(en.len() > 60);
        assert!(en.keys().all(|k| de.contains_key(k)), "every English text has a German one: {:?}", en.keys().find(|k| !de.contains_key(*k)));
        assert!(de.keys().all(|k| en.contains_key(k)), "every German text has an English one: {:?}", de.keys().find(|k| !en.contains_key(*k)));
    }

    #[test]
    fn default_key_is_valid() {
        assert!("CommandOrControl+Alt+Shift+W".parse::<Shortcut>().is_ok());
        assert!("Alt+Shift+Space".parse::<Shortcut>().is_ok());
    }
}
