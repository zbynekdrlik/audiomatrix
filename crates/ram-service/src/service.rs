//! Service coordinator.
//!
//! Manages the lifecycle of all `AudioMatrix` components.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use ram_api::{
    models::{DeviceStatus, DeviceType},
    router::create_router_with_state,
    websocket::{ErrorUpdate, MeteringUpdate, WsEvent},
    AppState, DeviceCommand,
};
use ram_core::device::DeviceManager;
use ram_discovery::{
    BroadcastAnnouncement, BroadcastDiscovery, BroadcastEvent, DiscoveryEvent, ServiceAnnouncer,
    ServiceBrowser,
};

use crate::audio_processor::AudioProcessor;
use crate::config::ServiceConfig;
use crate::device_state::DeviceStateManager;
use crate::vban_manager::{VbanManager, VbanManagerConfig};

/// Service shutdown signal.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    sender: broadcast::Sender<()>,
}

impl ShutdownSignal {
    /// Creates a new shutdown signal.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1);
        Self { sender }
    }

    /// Triggers a shutdown.
    pub fn shutdown(&self) {
        let _ = self.sender.send(());
    }

    /// Subscribes to the shutdown signal.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Service state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// Service is initializing.
    Initializing,
    /// Service is starting components.
    Starting,
    /// Service is running.
    Running,
    /// Service is shutting down.
    ShuttingDown,
    /// Service has stopped.
    Stopped,
}

/// The main `AudioMatrix` service.
pub struct AudioMatrixService {
    /// Service configuration.
    config: ServiceConfig,
    /// Application state shared with API.
    app_state: AppState,
    /// Device manager.
    device_manager: Arc<DeviceManager>,
    /// Audio processor for real-time audio routing.
    audio_processor: Arc<AudioProcessor>,
    /// VBAN manager for cross-node audio.
    vban_manager: Arc<VbanManager>,
    /// Device state persistence manager.
    device_state_manager: Arc<DeviceStateManager>,
    /// Service announcer for mDNS.
    announcer: Option<ServiceAnnouncer>,
    /// Service browser for discovery.
    browser: Option<ServiceBrowser>,
    /// Broadcast discovery (UDP fallback).
    broadcast_discovery: Option<Arc<BroadcastDiscovery>>,
    /// Shutdown signal.
    shutdown: ShutdownSignal,
    /// Current service state.
    state: Arc<RwLock<ServiceState>>,
    /// Shutdown flag for background tasks.
    running: Arc<AtomicBool>,
    /// API server task.
    api_task: Option<JoinHandle<()>>,
    /// Discovery event task.
    discovery_task: Option<JoinHandle<()>>,
    /// Broadcast discovery event task.
    broadcast_task: Option<JoinHandle<()>>,
    /// Metering broadcast task.
    metering_task: Option<JoinHandle<()>>,
    /// Device command handler task.
    device_command_task: Option<JoinHandle<()>>,
}

impl AudioMatrixService {
    /// Creates a new service with the given configuration.
    #[must_use]
    pub fn new(config: ServiceConfig) -> Self {
        let device_manager = Arc::new(DeviceManager::with_defaults());
        let audio_processor = Arc::new(AudioProcessor::with_defaults());

        // Create device state manager for persistence
        let data_dir = Self::get_data_dir();
        let device_state_manager = Arc::new(DeviceStateManager::new(data_dir));

        // Get the route controller from AudioProcessor for API integration
        let route_controller = audio_processor.route_controller();

        // Create AppState with the route controller wired in
        let app_state = AppState::with_route_controller(
            &config.node_name,
            config.api.port,
            config.vban.port,
            Some(route_controller),
        );

        // Create VBAN manager for cross-node audio
        let vban_config = VbanManagerConfig {
            local_port: config.vban.port,
            sample_rate: 48000,
        };
        let vban_manager = Arc::new(VbanManager::new(
            vban_config,
            Arc::clone(audio_processor.buffer_pool()),
            Arc::clone(audio_processor.routing_table()),
            Arc::clone(audio_processor.subscription_manager()),
        ));

        // Wire up VbanManager to RouteManager for API access
        audio_processor.set_vban_manager(Arc::clone(&vban_manager));

        // Set up stream starter for RouteController to start input streams when needed
        audio_processor.setup_stream_starter();
        // Set up output stream starter for cross-node routing (receiver side)
        audio_processor.setup_output_stream_starter();
        // Set up metering contexts for WebSocket broadcast
        audio_processor.setup_metering_contexts();

        Self {
            config,
            app_state,
            device_manager,
            audio_processor,
            vban_manager,
            device_state_manager,
            announcer: None,
            browser: None,
            broadcast_discovery: None,
            shutdown: ShutdownSignal::new(),
            state: Arc::new(RwLock::new(ServiceState::Initializing)),
            running: Arc::new(AtomicBool::new(false)),
            api_task: None,
            discovery_task: None,
            broadcast_task: None,
            metering_task: None,
            device_command_task: None,
        }
    }

    /// Gets the data directory for storing persistent state.
    fn get_data_dir() -> std::path::PathBuf {
        #[cfg(windows)]
        {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                return std::path::PathBuf::from(local_app_data).join("AudioMatrix");
            }
        }
        #[cfg(not(windows))]
        {
            if let Ok(home) = std::env::var("HOME") {
                return std::path::PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("audiomatrix");
            }
        }
        // Fallback to current directory
        std::path::PathBuf::from(".")
    }

    /// Creates a service with default configuration.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_defaults() -> Self {
        Self::new(ServiceConfig::default())
    }

    /// Returns the current service state.
    #[must_use]
    pub fn state(&self) -> ServiceState {
        *self.state.read()
    }

    /// Returns the application state.
    #[must_use]
    #[allow(dead_code)]
    pub fn app_state(&self) -> &AppState {
        &self.app_state
    }

    /// Returns a clone of the shutdown signal.
    #[must_use]
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown.clone()
    }

    /// Starts all service components.
    ///
    /// # Errors
    ///
    /// Returns an error if any component fails to start.
    pub async fn start(&mut self) -> Result<()> {
        *self.state.write() = ServiceState::Starting;
        self.running.store(true, Ordering::SeqCst);
        info!("Starting AudioMatrix service...");

        // Refresh devices
        info!("Refreshing audio devices...");
        self.device_manager.refresh();
        let devices = self.device_manager.devices();
        info!("Found {} audio devices", devices.len());

        // Register devices with API state
        for device in &devices {
            // Get first supported sample rate or default to 48000
            let sample_rate = device.configs.first().map_or(48000, |c| c.sample_rate_max);

            // Determine input/output channel counts based on device direction
            let (input_channels, output_channels, device_type) = match device.direction {
                ram_core::DeviceDirection::Input => (
                    device.max_input_channels(),
                    0,
                    ram_api::models::DeviceType::Input,
                ),
                ram_core::DeviceDirection::Output => (
                    0,
                    device.max_output_channels(),
                    ram_api::models::DeviceType::Output,
                ),
                ram_core::DeviceDirection::Duplex => (
                    device.max_input_channels(),
                    device.max_output_channels(),
                    ram_api::models::DeviceType::Duplex,
                ),
            };

            // Check for persisted state
            let persisted = self.device_state_manager.get_device_state(&device.id);

            // Apply persisted attached status, or use device's current status
            let status = if let Some(ref ps) = persisted {
                if ps.attached {
                    ram_api::models::DeviceStatus::Attached
                } else {
                    ram_api::models::DeviceStatus::Available
                }
            } else {
                match device.attachment_state {
                    ram_core::AttachmentState::Available => {
                        ram_api::models::DeviceStatus::Available
                    },
                    ram_core::AttachmentState::Attached => ram_api::models::DeviceStatus::Attached,
                    ram_core::AttachmentState::Active => ram_api::models::DeviceStatus::Active,
                    ram_core::AttachmentState::Detached => ram_api::models::DeviceStatus::Detached,
                    ram_core::AttachmentState::Error => ram_api::models::DeviceStatus::Error,
                }
            };

            // Apply persisted display name
            let display_name = persisted
                .as_ref()
                .and_then(|ps| ps.display_name.clone())
                .or_else(|| device.display_name.clone());

            // Apply persisted sample rate/buffer size
            let effective_sample_rate = persisted
                .as_ref()
                .and_then(|ps| ps.sample_rate)
                .unwrap_or(sample_rate);
            let effective_buffer_size = persisted
                .as_ref()
                .and_then(|ps| ps.buffer_size)
                .unwrap_or(self.config.audio.default_buffer_size);

            debug!(
                "Registering device: {} ({:?}) - {} in / {} out, attached={}",
                device.name,
                device_type,
                input_channels,
                output_channels,
                matches!(
                    status,
                    ram_api::models::DeviceStatus::Attached | ram_api::models::DeviceStatus::Active
                )
            );
            self.app_state.register_device(ram_api::models::DeviceInfo {
                id: device.id.clone(),
                name: device.name.clone(),
                display_name,
                device_type,
                input_channels,
                output_channels,
                sample_rate: effective_sample_rate,
                buffer_size: effective_buffer_size,
                is_virtual: device.is_virtual,
                status,
                backend: Some(device.host.clone()),
            });
        }
        info!(
            "Registered {} devices in API state",
            self.app_state.all_devices().len()
        );

        // Apply persisted channel labels
        for (device_id, labels) in self.device_state_manager.get_all_channel_labels() {
            for (channel, label) in labels {
                let _ = self.app_state.set_channel_label(&device_id, channel, label);
            }
        }

        // Start service discovery
        if self.config.discovery.enabled {
            self.start_discovery();
        }

        // Start API server
        self.start_api_server().await?;

        // Start audio processor
        self.audio_processor.start();
        info!("Audio processor started");

        // Start default audio streams ONLY if explicitly enabled (zero auto-connect policy)
        if self.config.audio.auto_start_devices {
            warn!("Auto-start devices is enabled - this bypasses zero auto-connect policy");
            self.start_default_streams();
        } else {
            info!("Zero auto-connect: No devices started automatically. Use Web UI to attach devices.");
        }

        // Restore persisted attached devices (start their streams)
        self.restore_attached_devices();

        // Start metering broadcast
        self.start_metering_broadcast();

        // Start device command handler for attach/detach stream control
        self.start_device_command_handler();

        // Start VBAN receiver for cross-node audio
        if let Err(e) = self.vban_manager.start_receiver().await {
            warn!("Failed to start VBAN receiver: {e}");
        } else {
            info!("VBAN receiver started on port {}", self.config.vban.port);
        }

        *self.state.write() = ServiceState::Running;
        info!("AudioMatrix service is running");

        Ok(())
    }

    /// Starts the service discovery components.
    fn start_discovery(&mut self) {
        info!("Starting service discovery...");

        // Start mDNS announcer (may not work reliably on all platforms)
        match ServiceAnnouncer::new(&self.config.node_name, self.config.api.port) {
            Ok(announcer) => {
                self.announcer = Some(announcer);
                info!("Service announcer started");
            },
            Err(e) => {
                warn!("Failed to start service announcer: {e}");
            },
        }

        // Start mDNS browser
        match ServiceBrowser::new() {
            Ok(browser) => {
                // Subscribe to discovery events
                let rx = browser.subscribe();
                let app_state = self.app_state.clone();
                let running = self.running.clone();

                // Spawn blocking task to handle discovery events
                let task = tokio::task::spawn_blocking(move || {
                    while running.load(Ordering::SeqCst) {
                        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                            Ok(event) => match event {
                                DiscoveryEvent::NodeDiscovered(node)
                                | DiscoveryEvent::NodeUpdated(node) => {
                                    info!(
                                        "Discovered node via mDNS: {} at {:?}",
                                        node.name, node.addresses
                                    );
                                    // Use node name as ID since discovery doesn't provide ID
                                    let id = format!("{}@{}", node.name, node.hostname);
                                    app_state.upsert_remote_node(ram_api::models::NodeInfo {
                                        id,
                                        name: node.name.clone(),
                                        addresses: node.addresses.clone(),
                                        api_port: node.port,
                                        vban_port: 6980, // Default VBAN port
                                        online: true,
                                    });
                                },
                                DiscoveryEvent::NodeRemoved { name } => {
                                    info!("Node removed: {name}");
                                    let _ = app_state.remove_remote_node(&name);
                                },
                                DiscoveryEvent::Error(err) => {
                                    warn!("Discovery error: {err}");
                                },
                            },
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                                // Continue polling
                            },
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                                break;
                            },
                        }
                    }
                });

                self.discovery_task = Some(task);
                self.browser = Some(browser);
                info!("Service browser started");
            },
            Err(e) => {
                warn!("Failed to start service browser: {e}");
            },
        }

        // Start UDP broadcast discovery (reliable fallback)
        self.start_broadcast_discovery();
    }

    /// Starts the UDP broadcast discovery fallback.
    fn start_broadcast_discovery(&mut self) {
        let announcement = BroadcastAnnouncement {
            name: self.config.node_name.clone(),
            api_port: self.config.api.port,
            vban_port: self.config.vban.port,
            input_channels: 0, // Will be updated when streams start
            output_channels: 0,
            sample_rate: 48000,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let discovery = Arc::new(BroadcastDiscovery::with_defaults(announcement));

        // Subscribe to broadcast events
        let rx = discovery.subscribe();
        let app_state = self.app_state.clone();
        let running = self.running.clone();

        // Spawn task to handle broadcast discovery events
        let task = tokio::task::spawn_blocking(move || {
            while running.load(Ordering::SeqCst) {
                match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(event) => match event {
                        BroadcastEvent::NodeDiscovered(node) => {
                            info!(
                                "Discovered node via broadcast: {} at {}:{}",
                                node.name, node.address, node.api_port
                            );
                            let id = format!("{}@{}", node.name, node.address);
                            app_state.upsert_remote_node(ram_api::models::NodeInfo {
                                id,
                                name: node.name.clone(),
                                addresses: vec![node.address.to_string()],
                                api_port: node.api_port,
                                vban_port: node.vban_port,
                                online: true,
                            });
                        },
                        BroadcastEvent::NodeTimeout(name) => {
                            info!("Node timed out: {name}");
                            // Don't remove immediately - mDNS might still have it
                        },
                    },
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        // Continue polling
                    },
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        break;
                    },
                }
            }
        });

        if let Err(e) = discovery.start() {
            warn!("Failed to start broadcast discovery: {e}");
        } else {
            self.broadcast_task = Some(task);
            self.broadcast_discovery = Some(discovery);
            info!("Broadcast discovery started");
        }
    }

    /// Starts the API server.
    async fn start_api_server(&mut self) -> Result<()> {
        let addr = SocketAddr::new(self.config.api.bind, self.config.api.port);
        info!("Starting API server on {addr}...");

        let app = create_router_with_state(self.app_state.clone());
        let listener = tokio::net::TcpListener::bind(addr).await?;

        let mut shutdown_rx = self.shutdown.subscribe();

        let task = tokio::spawn(async move {
            tokio::select! {
                result = axum::serve(listener, app) => {
                    if let Err(e) = result {
                        error!("API server error: {e}");
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("API server shutting down");
                }
            }
        });

        self.api_task = Some(task);
        info!("API server listening on {addr}");

        Ok(())
    }

    /// Starts audio streams for default devices.
    ///
    /// This is called during service startup to automatically begin audio
    /// routing with the system's default input and output devices.
    fn start_default_streams(&self) {
        // Get default devices
        let default_input = self.device_manager.default_input();
        let default_output = self.device_manager.default_output();

        // Start default input stream
        if let Some(device) = default_input {
            info!("Starting default input stream: {}", device.name);
            match self.audio_processor.start_input_stream(&device.id) {
                Ok(()) => info!("Default input stream started: {}", device.id),
                Err(e) => warn!("Failed to start default input stream: {e}"),
            }
        } else {
            info!("No default input device available");
        }

        // Start default output stream
        if let Some(device) = default_output {
            info!("Starting default output stream: {}", device.name);
            match self.audio_processor.start_output_stream(&device.id) {
                Ok(()) => info!("Default output stream started: {}", device.id),
                Err(e) => warn!("Failed to start default output stream: {e}"),
            }
        } else {
            info!("No default output device available");
        }

        let input_count = self.audio_processor.active_input_stream_count();
        let output_count = self.audio_processor.active_output_stream_count();
        info!(
            "Audio streams active: {} input, {} output",
            input_count, output_count
        );
    }

    /// Restores previously attached devices from persisted state.
    fn restore_attached_devices(&self) {
        let attached_devices = self.device_state_manager.get_attached_devices();

        if attached_devices.is_empty() {
            info!("No previously attached devices to restore");
            return;
        }

        info!(
            "Restoring {} previously attached devices",
            attached_devices.len()
        );

        for device_id in attached_devices {
            // Get device info to determine type
            if let Some(device) = self.app_state.get_device(&device_id) {
                info!("Restoring device: {} ({})", device.name, device_id);

                // Start input stream for devices with input capability
                if matches!(device.device_type, DeviceType::Input | DeviceType::Duplex) {
                    match self.audio_processor.start_input_stream(&device_id) {
                        Ok(()) => info!("Input stream restored for: {}", device_id),
                        Err(e) => warn!("Failed to restore input stream for {}: {e}", device_id),
                    }
                }

                // Start output stream for devices with output capability
                if matches!(device.device_type, DeviceType::Output | DeviceType::Duplex) {
                    match self.audio_processor.start_output_stream(&device_id) {
                        Ok(()) => info!("Output stream restored for: {}", device_id),
                        Err(e) => warn!("Failed to restore output stream for {}: {e}", device_id),
                    }
                }
            } else {
                warn!("Previously attached device no longer exists: {}", device_id);
                // Remove from persisted state since device is gone
                if let Err(e) = self
                    .device_state_manager
                    .set_device_attached(&device_id, false)
                {
                    warn!("Failed to update persisted state: {e}");
                }
            }
        }

        let input_count = self.audio_processor.active_input_stream_count();
        let output_count = self.audio_processor.active_output_stream_count();
        info!(
            "After restore: {} input streams, {} output streams active",
            input_count, output_count
        );
    }

    /// Starts the metering broadcast task.
    ///
    /// This task periodically reads meter levels from all active streams
    /// and broadcasts them via WebSocket to connected clients.
    fn start_metering_broadcast(&mut self) {
        // Get the thread-safe metering contexts (can be shared across threads)
        let metering_contexts = Arc::clone(self.audio_processor.metering_contexts());
        let app_state = self.app_state.clone();
        let running = self.running.clone();
        // Use full node ID (name@hostname) to match what the UI expects for subscriptions
        let node_id = app_state.local_node().id;

        // Use spawn_blocking for the metering loop
        let task = tokio::task::spawn_blocking(move || {
            // Broadcast metering at 30 Hz (every ~33ms)
            let interval = std::time::Duration::from_millis(33);

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(interval);

                // Collect input meters
                for (device_id, levels) in metering_contexts.all_input_meters() {
                    let db_levels: Vec<f32> = levels.iter().map(|l| l.rms_db).collect();
                    let db_peaks: Vec<f32> = levels.iter().map(|l| l.peak_db).collect();

                    // Only broadcast if there are non-silent levels
                    if db_levels.iter().any(|&l| l > -120.0) {
                        app_state.broadcast_event(WsEvent::Metering(MeteringUpdate {
                            node: node_id.clone(),
                            device: device_id,
                            direction: "input".to_string(),
                            levels: db_levels,
                            peaks: db_peaks,
                        }));
                    }
                }

                // Collect output meters
                for (device_id, levels) in metering_contexts.all_output_meters() {
                    let db_levels: Vec<f32> = levels.iter().map(|l| l.rms_db).collect();
                    let db_peaks: Vec<f32> = levels.iter().map(|l| l.peak_db).collect();

                    // Only broadcast if there are non-silent levels
                    if db_levels.iter().any(|&l| l > -120.0) {
                        app_state.broadcast_event(WsEvent::Metering(MeteringUpdate {
                            node: node_id.clone(),
                            device: device_id,
                            direction: "output".to_string(),
                            levels: db_levels,
                            peaks: db_peaks,
                        }));
                    }
                }
            }
        });

        self.metering_task = Some(task);
        info!("Metering broadcast started (30 Hz)");
    }

    /// Starts the device command handler task.
    ///
    /// This task listens for device attach/detach commands and
    /// starts/stops audio streams accordingly for metering support.
    /// Also persists device state on attach/detach.
    fn start_device_command_handler(&mut self) {
        let mut rx = self.app_state.subscribe_device_commands();
        let audio_processor = Arc::clone(&self.audio_processor);
        let device_state_manager = Arc::clone(&self.device_state_manager);
        let app_state = self.app_state.clone();
        let running = self.running.clone();

        let task = tokio::spawn(async move {
            info!("Device command handler task started, waiting for commands...");
            loop {
                tokio::select! {
                    result = rx.recv() => {
                        info!("Device command handler received message: {:?}", result);
                        match result {
                            Ok(command) => {
                                match command {
                                    DeviceCommand::StartStreams { device_id, device_type } => {
                                        info!("Processing StartStreams for device: {} (type: {:?})", device_id, device_type);

                                        // Persist attached state
                                        if let Err(e) = device_state_manager.set_device_attached(&device_id, true) {
                                            warn!("Failed to persist device attached state: {e}");
                                        }

                                        let mut input_ok = true;
                                        let mut output_ok = true;
                                        let mut error_msg = String::new();

                                        // Start input stream for devices with input capability
                                        if matches!(device_type, DeviceType::Input | DeviceType::Duplex) {
                                            match audio_processor.start_input_stream(&device_id) {
                                                Ok(()) => info!("Input stream started for: {device_id}"),
                                                Err(e) => {
                                                    error!("Failed to start input stream for {device_id}: {e}");
                                                    input_ok = false;
                                                    error_msg = format!("Input stream failed: {e}");
                                                }
                                            }
                                        }

                                        // Start output stream for devices with output capability
                                        if matches!(device_type, DeviceType::Output | DeviceType::Duplex) {
                                            match audio_processor.start_output_stream(&device_id) {
                                                Ok(()) => info!("Output stream started for: {device_id}"),
                                                Err(e) => {
                                                    error!("Failed to start output stream for {device_id}: {e}");
                                                    output_ok = false;
                                                    if !error_msg.is_empty() {
                                                        error_msg.push_str("; ");
                                                    }
                                                    use std::fmt::Write;
                                                    let _ = write!(error_msg, "Output stream failed: {e}");
                                                }
                                            }
                                        }

                                        // If both streams failed, update device status to error
                                        if !input_ok && !output_ok {
                                            error!("All streams failed for {device_id}, setting device to error state");
                                            app_state.set_device_status(&device_id, DeviceStatus::Error);
                                            app_state.broadcast_event(WsEvent::Error(ErrorUpdate {
                                                code: "STREAM_START_FAILED".to_string(),
                                                message: format!("Failed to start streams for {}: {}", device_id, error_msg),
                                            }));
                                        }
                                    }
                                    DeviceCommand::StopStreams { device_id } => {
                                        info!("Stopping streams for device: {device_id}");

                                        // Persist detached state
                                        if let Err(e) = device_state_manager.set_device_attached(&device_id, false) {
                                            warn!("Failed to persist device detached state: {e}");
                                        }

                                        audio_processor.stop_device_streams(&device_id);
                                    }
                                    DeviceCommand::ReconfigureStreams { device_id, device_type, sample_rate, buffer_size } => {
                                        info!("Reconfiguring streams for device: {device_id} (sample_rate: {sample_rate}, buffer_size: {buffer_size})");

                                        // Stop existing streams
                                        audio_processor.stop_device_streams(&device_id);

                                        // Start streams with new config
                                        // Track actual sample rates used (may differ from requested due to hardware limitations)
                                        let mut input_ok = true;
                                        let mut output_ok = true;
                                        let mut error_msg = String::new();
                                        let mut actual_sample_rate = sample_rate;

                                        if matches!(device_type, DeviceType::Input | DeviceType::Duplex) {
                                            match audio_processor.start_input_stream_with_config(&device_id, Some(sample_rate)) {
                                                Ok(actual_rate) => {
                                                    info!("Input stream restarted for: {device_id} at {actual_rate}Hz");
                                                    actual_sample_rate = actual_rate;
                                                }
                                                Err(e) => {
                                                    error!("Failed to restart input stream for {device_id}: {e}");
                                                    input_ok = false;
                                                    error_msg = format!("Input stream failed: {e}");
                                                }
                                            }
                                        }

                                        if matches!(device_type, DeviceType::Output | DeviceType::Duplex) {
                                            match audio_processor.start_output_stream_with_config(&device_id, Some(sample_rate)) {
                                                Ok(actual_rate) => {
                                                    info!("Output stream restarted for: {device_id} at {actual_rate}Hz");
                                                    actual_sample_rate = actual_rate;
                                                }
                                                Err(e) => {
                                                    error!("Failed to restart output stream for {device_id}: {e}");
                                                    output_ok = false;
                                                    if !error_msg.is_empty() {
                                                        error_msg.push_str("; ");
                                                    }
                                                    use std::fmt::Write;
                                                    let _ = write!(error_msg, "Output stream failed: {e}");
                                                }
                                            }
                                        }

                                        if !input_ok && !output_ok {
                                            error!("All streams failed to restart for {device_id}");
                                            app_state.set_device_status(&device_id, DeviceStatus::Error);
                                            app_state.broadcast_event(WsEvent::Error(ErrorUpdate {
                                                code: "STREAM_RECONFIGURE_FAILED".to_string(),
                                                message: format!("Failed to reconfigure streams for {}: {}", device_id, error_msg),
                                            }));
                                        } else {
                                            info!("Device {} reconfigured successfully at {}Hz", device_id, actual_sample_rate);

                                            // Persist actual config (may differ from requested if hardware doesn't support it)
                                            if let Err(e) = device_state_manager.set_device_config(&device_id, Some(actual_sample_rate), Some(buffer_size)) {
                                                warn!("Failed to persist device config: {e}");
                                            }

                                            // Update device state to reflect actual sample rate
                                            if actual_sample_rate != sample_rate {
                                                info!("Device {} sample rate adjusted from {} to {} due to hardware limitations",
                                                    device_id, sample_rate, actual_sample_rate);
                                                // Update device sample_rate in API state
                                                if let Err(e) = app_state.update_device(&device_id, None, Some(actual_sample_rate), None) {
                                                    warn!("Failed to update device state with actual sample rate: {e}");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Device command handler lagged by {n} messages");
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("Device command channel closed");
                                break;
                            }
                        }
                    }
                    () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        });

        self.device_command_task = Some(task);
        info!("Device command handler started");
    }

    /// Runs the service until shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the service fails.
    pub async fn run(&mut self) -> Result<()> {
        self.start().await?;

        // Wait for shutdown signal
        let mut shutdown_rx = self.shutdown.subscribe();
        shutdown_rx.recv().await.ok();

        self.stop().await;

        Ok(())
    }

    /// Stops all service components.
    pub async fn stop(&mut self) {
        if self.state() == ServiceState::Stopped {
            return;
        }

        *self.state.write() = ServiceState::ShuttingDown;
        info!("Stopping AudioMatrix service...");

        // Signal shutdown
        self.running.store(false, Ordering::SeqCst);
        self.shutdown.shutdown();

        // Stop VBAN manager
        self.vban_manager.stop_all().await;
        info!("VBAN manager stopped");

        // Stop audio processor first to ensure clean audio shutdown
        self.audio_processor.stop();
        info!("Audio processor stopped");

        // Stop announcer
        if let Some(announcer) = self.announcer.take() {
            if let Err(e) = announcer.shutdown() {
                warn!("Error shutting down announcer: {e}");
            }
            info!("Service announcer stopped");
        }

        // Stop browser
        if let Some(browser) = self.browser.take() {
            if let Err(e) = browser.shutdown() {
                warn!("Error shutting down browser: {e}");
            }
            info!("Service browser stopped");
        }

        // Stop broadcast discovery
        if let Some(broadcast) = self.broadcast_discovery.take() {
            broadcast.stop();
            info!("Broadcast discovery stopped");
        }

        // Wait for discovery task
        if let Some(task) = self.discovery_task.take() {
            let _ = task.await;
            info!("Discovery task stopped");
        }

        // Wait for broadcast task
        if let Some(task) = self.broadcast_task.take() {
            let _ = task.await;
            info!("Broadcast task stopped");
        }

        // Wait for metering task
        if let Some(task) = self.metering_task.take() {
            let _ = task.await;
            info!("Metering task stopped");
        }

        // Wait for API task
        if let Some(task) = self.api_task.take() {
            let _ = task.await;
            info!("API server stopped");
        }

        *self.state.write() = ServiceState::Stopped;
        info!("AudioMatrix service stopped");
    }
}

impl Drop for AudioMatrixService {
    fn drop(&mut self) {
        // Ensure shutdown is signaled
        self.running.store(false, Ordering::SeqCst);
        self.shutdown.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_signal_works() {
        let signal = ShutdownSignal::new();
        let mut rx = signal.subscribe();

        signal.shutdown();

        // Should receive the shutdown
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn service_state_initial() {
        let service = AudioMatrixService::with_defaults();
        assert_eq!(service.state(), ServiceState::Initializing);
    }

    #[test]
    fn service_config_applied() {
        let config = ServiceConfig::default()
            .with_node_name("TestNode")
            .with_api_port(9000);
        let service = AudioMatrixService::new(config);
        assert_eq!(service.app_state().local_node().name, "TestNode");
        assert_eq!(service.app_state().local_node().api_port, 9000);
    }

    #[tokio::test]
    async fn service_stop_without_start() {
        let mut service = AudioMatrixService::with_defaults();
        service.stop().await;
        assert_eq!(service.state(), ServiceState::Stopped);
    }
}
