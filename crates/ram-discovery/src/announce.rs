//! Service announcement via mDNS.

use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::{debug, info};

use crate::{Error, Result, SERVICE_TYPE};

/// Announces `AudioMatrix` service on the local network.
pub struct ServiceAnnouncer {
    daemon: ServiceDaemon,
    instance_name: String,
}

impl ServiceAnnouncer {
    /// Creates a new service announcer.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon or service registration fails.
    pub fn new(instance_name: &str, port: u16) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))?;

        let hostname =
            hostname::get().map_or_else(|_| "unknown".into(), |h| h.to_string_lossy().into_owned());

        let service_info = ServiceInfo::new(SERVICE_TYPE, instance_name, &hostname, (), port, None)
            .map_err(|e| Error::Registration(e.to_string()))?;

        daemon
            .register(service_info)
            .map_err(|e| Error::Registration(e.to_string()))?;

        info!("Announced service: {instance_name} on port {port}");

        Ok(Self {
            daemon,
            instance_name: instance_name.to_string(),
        })
    }

    /// Returns the instance name.
    #[must_use]
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    /// Stops announcing the service.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon fails to shut down.
    pub fn shutdown(self) -> Result<()> {
        debug!("Shutting down service announcer");
        self.daemon
            .shutdown()
            .map_err(|e| Error::Mdns(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // mDNS tests require network access and are flaky in CI
    // Manual testing recommended
}
