//! Windows system tray integration.
//!
//! Provides a system tray icon with menu for:
//! - Dynamic status icon (green=running, yellow=starting, red=offline)
//! - Version display in tooltip
//! - Open Web UI
//! - View stats (routes, streams, nodes)
//! - View logs
//! - Check for updates
//! - Exit application
//!
//! Inspired by the mature implementation in DanteSync.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tracing::{info, warn};
use tray_icon::{Icon, TrayIconBuilder};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;
use winrt_notification::{Sound, Toast};

use crate::service::ShutdownSignal;

// ============================================================================
// SINGLE INSTANCE GUARD - Prevent multiple tray apps
// ============================================================================

struct SingleInstanceGuard {
    _handle: HANDLE,
}

impl SingleInstanceGuard {
    /// Try to acquire single-instance lock. Returns None if another instance is running.
    fn try_acquire() -> Option<Self> {
        unsafe {
            let mutex_name: Vec<u16> = "Global\\AudioMatrixTrayMutex\0".encode_utf16().collect();
            let handle = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()));

            match handle {
                Ok(h) => {
                    // Check if mutex already existed
                    let last_error = windows::Win32::Foundation::GetLastError();
                    if last_error == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
                        let _ = CloseHandle(h);
                        return None;
                    }
                    Some(SingleInstanceGuard { _handle: h })
                },
                Err(_) => None,
            }
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self._handle);
        }
    }
}

// ============================================================================
// APP EVENTS
// ============================================================================

#[derive(Debug)]
enum AppEvent {
    StatusUpdate(ServiceStatus),
    Offline,
    NewVersionAvailable(String),
}

#[derive(Debug, Clone, Default)]
struct ServiceStatus {
    routes: usize,
    input_streams: usize,
    output_streams: usize,
    remote_nodes: usize,
    healthy: bool,
}

// ============================================================================
// GITHUB RELEASE - For version check
// ============================================================================

#[derive(serde::Deserialize, Debug)]
struct GitHubRelease {
    tag_name: String,
}

const GITHUB_API_URL: &str = "https://api.github.com/repos/zbynekdrlik/audiomatrix/releases/latest";

/// Parse version string (e.g., "v0.1.0" or "0.1.0-dev.7") into comparable parts
fn parse_version(version: &str) -> Option<(u32, u32, u32, Option<u32>)> {
    let v = version.trim_start_matches('v');
    let (base, dev) = if let Some(pos) = v.find("-dev.") {
        let dev_num: u32 = v[pos + 5..].parse().ok()?;
        (&v[..pos], Some(dev_num))
    } else {
        (v, None)
    };

    let parts: Vec<&str> = base.split('.').collect();
    if parts.len() >= 3 {
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
            dev,
        ))
    } else {
        None
    }
}

/// Compare versions, returns true if remote is newer than local
fn is_newer_version(local: &str, remote: &str) -> bool {
    match (parse_version(local), parse_version(remote)) {
        (Some(l), Some(r)) => r > l,
        _ => false,
    }
}

/// Fetch the latest version from GitHub releases
async fn check_latest_version() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("AudioMatrix-Tray")
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(GITHUB_API_URL).send().await?;
    let release: GitHubRelease = response.json().await?;
    Ok(release.tag_name)
}

// ============================================================================
// TOAST NOTIFICATIONS
// ============================================================================

fn show_notification(title: &str, message: &str) {
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(message)
        .sound(Some(Sound::Default))
        .show();
}

// ============================================================================
// ICON GENERATION
// ============================================================================

/// Generate a dynamic icon with optional update badge
fn generate_icon(r: u8, g: u8, b: u8, show_update_badge: bool) -> Icon {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    let cx = 16.0f32;
    let cy = 16.0f32;
    let radius = 12.0f32;

    // Update badge position
    let badge_cx = 25.0f32;
    let badge_cy = 7.0f32;
    let badge_radius = 5.0f32;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx + 0.5;
            let dy = y as f32 - cy + 0.5;
            let dist = (dx * dx + dy * dy).sqrt();

            // Check if pixel is in update badge area
            let badge_dx = x as f32 - badge_cx + 0.5;
            let badge_dy = y as f32 - badge_cy + 0.5;
            let badge_dist = (badge_dx * badge_dx + badge_dy * badge_dy).sqrt();

            if show_update_badge && badge_dist <= badge_radius {
                // Update badge - orange color
                let mut alpha = 255u8;
                if badge_dist > badge_radius - 1.0 {
                    alpha = ((badge_radius - badge_dist) * 255.0).max(0.0) as u8;
                }
                rgba.push(255); // R - orange
                rgba.push(152); // G
                rgba.push(0); // B
                rgba.push(alpha);
            } else if dist <= radius {
                // Main fill
                let mut alpha = 255u8;
                if dist > radius - 1.0 {
                    alpha = ((radius - dist) * 255.0).max(0.0) as u8;
                }
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(alpha);
            } else {
                // Transparent
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }
    Icon::from_rgba(rgba, width, height).unwrap()
}

// ============================================================================
// TRAY APPLICATION
// ============================================================================

struct TrayApp {
    shutdown: ShutdownSignal,
    api_port: u16,
    running: Arc<AtomicBool>,
    update_available: Arc<AtomicBool>,

    // Menu items
    status_item: MenuItem,
    stats_item: MenuItem,
    upgrade_item: MenuItem,
    quit_item: MenuItem,
    open_ui_item: MenuItem,
    view_logs_item: MenuItem,

    // Tray icon (RefCell to allow explicit drop)
    tray_icon: RefCell<Option<tray_icon::TrayIcon>>,

    // State
    was_online: bool,
}

impl TrayApp {
    fn new(
        shutdown: ShutdownSignal,
        api_port: u16,
        running: Arc<AtomicBool>,
        update_available: Arc<AtomicBool>,
    ) -> Self {
        let version = env!("CARGO_PKG_VERSION");

        // Create menu items
        let version_item = MenuItem::new(format!("AudioMatrix v{version}"), false, None);
        let status_item = MenuItem::new("Status: Starting...", false, None);
        let stats_item = MenuItem::new("Routes: -- | Streams: --", false, None);
        let open_ui_item = MenuItem::new("Open Web UI", true, None);
        let view_logs_item = MenuItem::new("View Logs...", true, None);
        let upgrade_item = MenuItem::new("Check for Updates...", true, None);
        let quit_item = MenuItem::new("Exit", true, None);

        // Build menu
        let menu = Menu::new();
        menu.append(&version_item).unwrap();
        menu.append(&status_item).unwrap();
        menu.append(&stats_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&open_ui_item).unwrap();
        menu.append(&view_logs_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&upgrade_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        // Create initial icon (yellow - starting)
        let icon = generate_icon(255, 193, 7, false);
        let tooltip = format!("AudioMatrix v{version}\nStarting...");

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&tooltip)
            .with_icon(icon)
            .build()
            .unwrap();

        Self {
            shutdown,
            api_port,
            running,
            update_available,
            status_item,
            stats_item,
            upgrade_item,
            quit_item,
            open_ui_item,
            view_logs_item,
            tray_icon: RefCell::new(Some(tray_icon)),
            was_online: false,
        }
    }

    fn update_status(&mut self, status: ServiceStatus) {
        let version = env!("CARGO_PKG_VERSION");
        let has_update = self.update_available.load(Ordering::Relaxed);

        // Update icon color based on status
        let icon = if status.healthy {
            generate_icon(40, 167, 69, has_update) // Green - healthy
        } else {
            generate_icon(255, 193, 7, has_update) // Yellow - degraded
        };

        // Update tooltip
        let tooltip = format!(
            "AudioMatrix v{version}\nRoutes: {} | Nodes: {}\nStreams: {} in / {} out",
            status.routes, status.remote_nodes, status.input_streams, status.output_streams
        );

        // Update menu items
        self.status_item.set_text("Status: Running");
        self.stats_item.set_text(format!(
            "Routes: {} | Streams: {}/{}",
            status.routes, status.input_streams, status.output_streams
        ));

        if let Some(ref ti) = *self.tray_icon.borrow() {
            let _ = ti.set_icon(Some(icon));
            let _ = ti.set_tooltip(Some(tooltip));
        }

        // Show notification on first connect
        if !self.was_online {
            show_notification("AudioMatrix", "Service running");
            self.was_online = true;
        }
    }

    fn set_offline(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        let has_update = self.update_available.load(Ordering::Relaxed);

        let icon = generate_icon(220, 53, 69, has_update); // Red - offline
        let tooltip = format!("AudioMatrix v{version}\nService Offline");

        self.status_item.set_text("Status: Offline");
        self.stats_item.set_text("--");

        if let Some(ref ti) = *self.tray_icon.borrow() {
            let _ = ti.set_icon(Some(icon));
            let _ = ti.set_tooltip(Some(tooltip));
        }

        if self.was_online {
            show_notification("AudioMatrix", "Service offline");
            self.was_online = false;
        }
    }

    fn set_update_available(&mut self, new_version: &str) {
        self.upgrade_item
            .set_text(format!("Upgrade to {new_version}"));
        show_notification(
            "AudioMatrix",
            &format!("New version {new_version} available"),
        );
    }

    fn open_web_ui(&self) {
        let url = format!("http://127.0.0.1:{}", self.api_port);
        info!("Opening web UI: {url}");
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn();
    }

    fn view_logs(&self) {
        // Try to open log directory
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let log_dir = std::path::PathBuf::from(local_app_data)
                .join("AudioMatrix")
                .join("logs");

            if log_dir.exists() {
                let _ = std::process::Command::new("explorer").arg(&log_dir).spawn();
                return;
            }
        }
        // Fallback: open current directory
        let _ = std::process::Command::new("explorer").arg(".").spawn();
    }

    fn check_updates(&self) {
        let url = "https://github.com/zbynekdrlik/audiomatrix/releases/latest";
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn();
    }

    fn exit(&mut self) {
        info!("Exit requested from tray");
        self.running.store(false, Ordering::SeqCst);
        // Clean up tray icon
        self.tray_icon.borrow_mut().take();
        self.shutdown.shutdown();
    }

    fn handle_menu_event(&mut self, event_loop: &ActiveEventLoop) {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.quit_item.id() {
                self.exit();
                event_loop.exit();
            } else if event.id == self.open_ui_item.id() {
                self.open_web_ui();
            } else if event.id == self.view_logs_item.id() {
                self.view_logs();
            } else if event.id == self.upgrade_item.id() {
                self.check_updates();
            }
        }
    }
}

impl ApplicationHandler<AppEvent> for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Not used for tray apps
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // Not used for tray apps
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::StatusUpdate(status) => {
                self.update_status(status);
            },
            AppEvent::Offline => {
                self.set_offline();
            },
            AppEvent::NewVersionAvailable(version) => {
                self.set_update_available(&version);
            },
        }
        self.handle_menu_event(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + Duration::from_millis(100),
        ));

        // Check menu events
        self.handle_menu_event(event_loop);

        // Check if we should exit
        if !self.running.load(Ordering::SeqCst) {
            event_loop.exit();
        }
    }
}

// ============================================================================
// PUBLIC API
// ============================================================================

/// Run the system tray application.
///
/// This function blocks and runs the Windows event loop.
/// Call this from a dedicated thread.
pub fn run_tray(shutdown: ShutdownSignal, api_port: u16) {
    // Single-instance check
    let _guard = match SingleInstanceGuard::try_acquire() {
        Some(guard) => guard,
        None => {
            warn!("Another AudioMatrix tray instance is already running");
            return;
        },
    };

    info!("Starting system tray");

    // Set up shutdown detection
    let shutdown_rx = shutdown.subscribe();
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    // Spawn shutdown listener
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let mut rx = shutdown_rx;
            let _ = rx.recv().await;
            shutdown_flag_clone.store(true, Ordering::SeqCst);
        });
    });

    let event_loop = match EventLoop::<AppEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            warn!("Failed to create event loop (no GUI session?): {}", e);
            // Keep the thread alive but do nothing - service runs headlessly
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if shutdown_flag.load(Ordering::SeqCst) {
                    return;
                }
            }
        },
    };

    let proxy = event_loop.create_proxy();
    let running = Arc::new(AtomicBool::new(true));
    let update_available = Arc::new(AtomicBool::new(false));

    // Spawn status polling thread
    let status_proxy = proxy.clone();
    let status_running = running.clone();
    let status_port = api_port;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap();

            while status_running.load(Ordering::SeqCst) {
                // Poll API for status
                let health_url = format!("http://127.0.0.1:{}/api/v1/health", status_port);
                match client.get(&health_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        // Get route count
                        let routes_url = format!("http://127.0.0.1:{}/api/v1/routes", status_port);
                        let routes: usize = match client.get(&routes_url).send().await {
                            Ok(r) => r
                                .json::<Vec<serde_json::Value>>()
                                .await
                                .map(|v| v.len())
                                .unwrap_or(0),
                            Err(_) => 0,
                        };

                        // Get stream count
                        let streams_url =
                            format!("http://127.0.0.1:{}/api/v1/streams/count", status_port);
                        let (input_streams, output_streams) =
                            match client.get(&streams_url).send().await {
                                Ok(r) => match r.json::<serde_json::Value>().await {
                                    Ok(v) => (
                                        v.get("input").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as usize,
                                        v.get("output").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as usize,
                                    ),
                                    Err(_) => (0, 0),
                                },
                                Err(_) => (0, 0),
                            };

                        // Get node count
                        let nodes_url = format!("http://127.0.0.1:{}/api/v1/nodes", status_port);
                        let remote_nodes: usize = match client.get(&nodes_url).send().await {
                            Ok(r) => r.json::<Vec<serde_json::Value>>().await
                                .map(|v| v.len().saturating_sub(1)) // Exclude local node
                                .unwrap_or(0),
                            Err(_) => 0,
                        };

                        let _ = status_proxy.send_event(AppEvent::StatusUpdate(ServiceStatus {
                            routes,
                            input_streams,
                            output_streams,
                            remote_nodes,
                            healthy: true,
                        }));
                    },
                    _ => {
                        let _ = status_proxy.send_event(AppEvent::Offline);
                    },
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    });

    // Spawn version check thread
    let version_proxy = proxy.clone();
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let version_update_available = update_available.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            // Initial delay
            tokio::time::sleep(Duration::from_secs(30)).await;

            loop {
                if let Ok(latest) = check_latest_version().await {
                    if is_newer_version(&current_version, &latest)
                        && !version_update_available.load(Ordering::Relaxed)
                    {
                        version_update_available.store(true, Ordering::Relaxed);
                        let _ = version_proxy.send_event(AppEvent::NewVersionAvailable(latest));
                    }
                }
                // Check every 6 hours
                tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
            }
        });
    });

    let mut app = TrayApp::new(shutdown, api_port, running, update_available);

    event_loop.run_app(&mut app).expect("Event loop failed");

    info!("System tray exited");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_release() {
        assert_eq!(parse_version("v0.1.0"), Some((0, 1, 0, None)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3, None)));
    }

    #[test]
    fn parse_version_dev() {
        assert_eq!(parse_version("0.1.0-dev.7"), Some((0, 1, 0, Some(7))));
        assert_eq!(parse_version("v0.2.0-dev.1"), Some((0, 2, 0, Some(1))));
    }

    #[test]
    fn version_comparison() {
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "0.1.1"));
        assert!(is_newer_version("0.1.0-dev.5", "0.1.0"));
        assert!(!is_newer_version("0.2.0", "0.1.0"));
    }
}
