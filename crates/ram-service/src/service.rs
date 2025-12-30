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
use tracing::{error, info, warn};

use ram_api::{
    router::create_router_with_state,
    websocket::{MeteringUpdate, WsEvent},
    AppState,
};
use ram_core::device::DeviceManager;
use ram_discovery::{
    BroadcastAnnouncement, BroadcastDiscovery, BroadcastEvent, DiscoveryEvent, ServiceAnnouncer,
    ServiceBrowser,
};

use crate::audio_processor::AudioProcessor;
use crate::config::ServiceConfig;
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
}

impl AudioMatrixService {
    /// Creates a new service with the given configuration.
    #[must_use]
    pub fn new(config: ServiceConfig) -> Self {
        let device_manager = Arc::new(DeviceManager::with_defaults());
        let audio_processor = Arc::new(AudioProcessor::with_defaults());

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
        }
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
                ram_core::DeviceDirection::Input => {
                    (device.max_channels(), 0, ram_api::models::DeviceType::Input)
                },
                ram_core::DeviceDirection::Output => (
                    0,
                    device.max_channels(),
                    ram_api::models::DeviceType::Output,
                ),
            };

            // Convert attachment state to API device status
            let status = match device.attachment_state {
                ram_core::AttachmentState::Available => ram_api::models::DeviceStatus::Available,
                ram_core::AttachmentState::Attached => ram_api::models::DeviceStatus::Attached,
                ram_core::AttachmentState::Active => ram_api::models::DeviceStatus::Active,
                ram_core::AttachmentState::Detached => ram_api::models::DeviceStatus::Detached,
                ram_core::AttachmentState::Error => ram_api::models::DeviceStatus::Error,
            };

            self.app_state.register_device(ram_api::models::DeviceInfo {
                id: device.id.clone(),
                name: device.name.clone(),
                display_name: device.display_name.clone(),
                device_type,
                input_channels,
                output_channels,
                sample_rate,
                buffer_size: self.config.audio.default_buffer_size,
                is_virtual: device.is_virtual,
                status,
                backend: Some(device.host.clone()),
            });
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

        // Start metering broadcast
        self.start_metering_broadcast();

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

    /// Starts the metering broadcast task.
    ///
    /// This task periodically reads meter levels from all active streams
    /// and broadcasts them via WebSocket to connected clients.
    fn start_metering_broadcast(&mut self) {
        // Get the thread-safe metering contexts (can be shared across threads)
        let metering_contexts = Arc::clone(self.audio_processor.metering_contexts());
        let app_state = self.app_state.clone();
        let running = self.running.clone();
        let node_name = self.config.node_name.clone();

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
                            node: node_name.clone(),
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
                            node: node_name.clone(),
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
