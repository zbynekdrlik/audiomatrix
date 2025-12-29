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
use winit::platform::windows::EventLoopBuilderExtWindows;
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
/// In semver, release versions are newer than pre-releases with same major.minor.patch
fn is_newer_version(local: &str, remote: &str) -> bool {
    match (parse_version(local), parse_version(remote)) {
        (Some((lmaj, lmin, lpat, ldev)), Some((rmaj, rmin, rpat, rdev))) => {
            // Compare major.minor.patch first
            match (rmaj.cmp(&lmaj), rmin.cmp(&lmin), rpat.cmp(&lpat)) {
                (std::cmp::Ordering::Greater, _, _) => true,
                (std::cmp::Ordering::Less, _, _) => false,
                (_, std::cmp::Ordering::Greater, _) => true,
                (_, std::cmp::Ordering::Less, _) => false,
                (_, _, std::cmp::Ordering::Greater) => true,
                (_, _, std::cmp::Ordering::Less) => false,
                // Same major.minor.patch - compare pre-release
                // None (release) is newer than Some(_) (pre-release)
                (_, _, std::cmp::Ordering::Equal) => match (ldev, rdev) {
                    (Some(_), None) => true,         // local is pre-release, remote is release
                    (None, Some(_)) => false,        // local is release, remote is pre-release
                    (Some(ld), Some(rd)) => rd > ld, // both pre-release, compare dev numbers
                    (None, None) => false,           // same release version
                },
            }
        },
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
// ICON GENERATION - Unique 3x3 Grid with Diamonds
// ============================================================================

/// Tray icon state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconState {
    /// Starting up (yellow center diamond).
    Starting,
    /// Running with no routes (blue center diamond).
    Idle,
    /// Running with active routes (green pattern of diamonds).
    Active,
    /// Offline/error (red X).
    Error,
}

/// RGBA color type.
type Rgba = (u8, u8, u8, u8);

/// Color palette for icons.
mod colors {
    use super::Rgba;

    pub const GRID_LINE: Rgba = (100, 100, 100, 255);
    pub const GRID_BG: Rgba = (30, 30, 30, 255);

    // Diamond colors
    pub const BLUE: Rgba = (66, 135, 245, 255); // Idle
    pub const GREEN: Rgba = (40, 167, 69, 255); // Active
    pub const YELLOW: Rgba = (255, 193, 7, 255); // Starting
    pub const RED: Rgba = (220, 53, 69, 255); // Error

    // Update badge
    pub const ORANGE: Rgba = (255, 152, 0, 255);
}

/// Generate a unique 3x3 grid tray icon with diamond symbols.
///
/// Design distinct from DanteSync:
/// - 3x3 grid structure
/// - Diamond symbols indicate state
///
/// ```text
/// Starting: [   ][   ][   ]    Idle:    [   ][   ][   ]
///           [   ][ ◆ ][   ]             [   ][ ◆ ][   ]
///           [   ][   ][   ]             [   ][   ][   ]
///
/// Active:   [ ◆ ][   ][ ◆ ]    Error:   [ \ ][   ][ / ]
///           [   ][ ◆ ][   ]             [   ][ X ][   ]
///           [ ◆ ][   ][ ◆ ]             [ / ][   ][ \ ]
/// ```
fn generate_icon(state: TrayIconState, show_update_badge: bool) -> Icon {
    const SIZE: u32 = 32;
    const CELL_SIZE: u32 = 10; // Each grid cell
    const GRID_START: u32 = 1; // Offset from edge
    const LINE_WIDTH: u32 = 1;

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    // Helper to set pixel
    let set_pixel = |rgba: &mut [u8], x: u32, y: u32, color: Rgba| {
        if x < SIZE && y < SIZE {
            let idx = ((y * SIZE + x) * 4) as usize;
            rgba[idx] = color.0;
            rgba[idx + 1] = color.1;
            rgba[idx + 2] = color.2;
            rgba[idx + 3] = color.3;
        }
    };

    // Draw background and grid
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Check if we're in the grid area
            let gx = x.saturating_sub(GRID_START);
            let gy = y.saturating_sub(GRID_START);

            if x >= GRID_START
                && x < GRID_START + CELL_SIZE * 3 + LINE_WIDTH * 2
                && y >= GRID_START
                && y < GRID_START + CELL_SIZE * 3 + LINE_WIDTH * 2
            {
                // Check if on grid line
                let on_vline = gx == CELL_SIZE || gx == CELL_SIZE * 2 + LINE_WIDTH;
                let on_hline = gy == CELL_SIZE || gy == CELL_SIZE * 2 + LINE_WIDTH;

                if on_vline || on_hline {
                    set_pixel(&mut rgba, x, y, colors::GRID_LINE);
                } else {
                    set_pixel(&mut rgba, x, y, colors::GRID_BG);
                }
            }
        }
    }

    // Helper to draw diamond in a cell (0-2, 0-2)
    let draw_diamond = |rgba: &mut [u8], cell_x: u32, cell_y: u32, color: Rgba| {
        // Calculate cell center
        let cx = GRID_START + cell_x * (CELL_SIZE + LINE_WIDTH) + CELL_SIZE / 2;
        let cy = GRID_START + cell_y * (CELL_SIZE + LINE_WIDTH) + CELL_SIZE / 2;
        let half = 3u32; // Half-size of diamond

        // Diamond shape (rotated square)
        for dy in 0..=half * 2 {
            for dx in 0..=half * 2 {
                let lx = dx as i32 - half as i32;
                let ly = dy as i32 - half as i32;
                // Manhattan distance for diamond shape
                if lx.abs() + ly.abs() <= half as i32 {
                    let px = (cx as i32 + lx) as u32;
                    let py = (cy as i32 + ly) as u32;
                    set_pixel(rgba, px, py, color);
                }
            }
        }
    };

    // Helper to draw X in a cell
    let draw_x = |rgba: &mut [u8], cell_x: u32, cell_y: u32, color: Rgba| {
        let start_x = GRID_START + cell_x * (CELL_SIZE + LINE_WIDTH) + 2;
        let start_y = GRID_START + cell_y * (CELL_SIZE + LINE_WIDTH) + 2;
        let size = CELL_SIZE - 4;

        for i in 0..size {
            // Main diagonal
            set_pixel(rgba, start_x + i, start_y + i, color);
            // Anti-diagonal
            set_pixel(rgba, start_x + size - 1 - i, start_y + i, color);
        }
    };

    // Draw state-specific pattern
    match state {
        TrayIconState::Starting => {
            // Yellow center diamond
            draw_diamond(&mut rgba, 1, 1, colors::YELLOW);
        },
        TrayIconState::Idle => {
            // Blue center diamond
            draw_diamond(&mut rgba, 1, 1, colors::BLUE);
        },
        TrayIconState::Active => {
            // Green diamonds in X pattern
            draw_diamond(&mut rgba, 0, 0, colors::GREEN); // Top-left
            draw_diamond(&mut rgba, 2, 0, colors::GREEN); // Top-right
            draw_diamond(&mut rgba, 1, 1, colors::GREEN); // Center
            draw_diamond(&mut rgba, 0, 2, colors::GREEN); // Bottom-left
            draw_diamond(&mut rgba, 2, 2, colors::GREEN); // Bottom-right
        },
        TrayIconState::Error => {
            // Red X pattern
            draw_x(&mut rgba, 0, 0, colors::RED);
            draw_x(&mut rgba, 2, 0, colors::RED);
            draw_x(&mut rgba, 1, 1, colors::RED);
            draw_x(&mut rgba, 0, 2, colors::RED);
            draw_x(&mut rgba, 2, 2, colors::RED);
        },
    }

    // Draw update badge (orange circle in top-right)
    if show_update_badge {
        let badge_cx = 26u32;
        let badge_cy = 6u32;
        let badge_r = 4u32;

        for y in badge_cy.saturating_sub(badge_r)..=badge_cy + badge_r {
            for x in badge_cx.saturating_sub(badge_r)..=badge_cx + badge_r {
                let dx = x as i32 - badge_cx as i32;
                let dy = y as i32 - badge_cy as i32;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= (badge_r * badge_r) as i32 {
                    set_pixel(&mut rgba, x, y, colors::ORANGE);
                }
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).expect("Failed to create icon")
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
    ) -> Result<Self, tray_icon::Error> {
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
        let icon = generate_icon(TrayIconState::Starting, false);
        let tooltip = format!("AudioMatrix v{version}\nStarting...");

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&tooltip)
            .with_icon(icon)
            .build()?;

        Ok(Self {
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
        })
    }

    fn update_status(&mut self, status: ServiceStatus) {
        let version = env!("CARGO_PKG_VERSION");
        let has_update = self.update_available.load(Ordering::Relaxed);

        // Update icon based on status
        // Use Active (green diamonds) if routes > 0, otherwise Idle (blue center)
        let icon = if status.healthy {
            if status.routes > 0 {
                generate_icon(TrayIconState::Active, has_update)
            } else {
                generate_icon(TrayIconState::Idle, has_update)
            }
        } else {
            generate_icon(TrayIconState::Starting, has_update) // Yellow - degraded
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

        let icon = generate_icon(TrayIconState::Error, has_update);
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

    let event_loop = match EventLoop::<AppEvent>::with_user_event()
        .with_any_thread(true) // Required for running on non-main thread
        .build()
    {
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

    let mut app = match TrayApp::new(
        shutdown.clone(),
        api_port,
        running.clone(),
        update_available,
    ) {
        Ok(app) => app,
        Err(e) => {
            warn!("Failed to create tray icon (no GUI session?): {}", e);
            // Run headlessly - just wait for shutdown
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if shutdown_flag.load(Ordering::SeqCst) {
                    return;
                }
            }
        },
    };

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

    #[test]
    fn tray_icon_state_coverage() {
        // Verify all states can be generated without panic
        let states = [
            TrayIconState::Starting,
            TrayIconState::Idle,
            TrayIconState::Active,
            TrayIconState::Error,
        ];

        for state in states {
            // Without update badge
            let _icon = generate_icon(state, false);
            // With update badge
            let _icon = generate_icon(state, true);
        }
    }
}
