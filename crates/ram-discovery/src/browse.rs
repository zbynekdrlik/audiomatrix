//! Service browsing via mDNS.

use std::collections::HashMap;
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{Error, Result, SERVICE_TYPE};

/// Information about a discovered `AudioMatrix` node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node name.
    pub name: String,
    /// Hostname.
    pub hostname: String,
    /// IP addresses.
    pub addresses: Vec<String>,
    /// API port.
    pub port: u16,
}

/// Browses for `AudioMatrix` services on the local network.
pub struct ServiceBrowser {
    daemon: ServiceDaemon,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
}

impl ServiceBrowser {
    /// Creates a new service browser.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be created.
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))?;

        Ok(Self {
            daemon,
            nodes: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Starts browsing for services.
    ///
    /// # Errors
    ///
    /// Returns an error if browsing cannot be started.
    pub fn start(&self) -> Result<()> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| Error::Mdns(e.to_string()))?;

        let nodes = self.nodes.clone();

        // Spawn a task to handle discovery events
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let node = NodeInfo {
                            name: info.get_fullname().to_string(),
                            hostname: info.get_hostname().to_string(),
                            addresses: info
                                .get_addresses()
                                .iter()
                                .map(std::string::ToString::to_string)
                                .collect(),
                            port: info.get_port(),
                        };
                        info!("Discovered node: {}", node.name);
                        nodes.write().insert(node.name.clone(), node);
                    },
                    ServiceEvent::ServiceRemoved(_, name) => {
                        debug!("Node removed: {name}");
                        nodes.write().remove(&name);
                    },
                    _ => {},
                }
            }
        });

        Ok(())
    }

    /// Returns all discovered nodes.
    #[must_use]
    pub fn nodes(&self) -> Vec<NodeInfo> {
        self.nodes.read().values().cloned().collect()
    }

    /// Returns a specific node by name.
    #[must_use]
    pub fn node(&self, name: &str) -> Option<NodeInfo> {
        self.nodes.read().get(name).cloned()
    }

    /// Stops browsing.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon fails to shut down.
    pub fn shutdown(self) -> Result<()> {
        self.daemon
            .shutdown()
            .map_err(|e| Error::Mdns(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // mDNS tests require network access and are flaky in CI
}
