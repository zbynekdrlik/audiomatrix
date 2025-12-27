//! Audio routing matrix.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::buffer::RingBuffer;
use crate::mixer::VolumeControl;
use crate::{Error, Result};

/// Unique identifier for a routing endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EndpointId {
    /// Node identifier (hostname or IP).
    pub node: String,
    /// Device name.
    pub device: String,
    /// Channel number (1-based).
    pub channel: u16,
}

impl EndpointId {
    /// Creates a new endpoint identifier.
    #[must_use]
    pub fn new(node: impl Into<String>, device: impl Into<String>, channel: u16) -> Self {
        Self {
            node: node.into(),
            device: device.into(),
            channel,
        }
    }

    /// Creates an endpoint for the local node.
    #[must_use]
    pub fn local(device: impl Into<String>, channel: u16) -> Self {
        Self::new("LOCAL", device, channel)
    }
}

impl std::fmt::Display for EndpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.node, self.device, self.channel)
    }
}

/// A single route from source to destination.
pub struct Route {
    /// Source endpoint.
    pub source: EndpointId,
    /// Destination endpoint.
    pub destination: EndpointId,
    /// Volume control for this route.
    pub volume: VolumeControl,
    /// Buffer for audio data in transit.
    buffer: Arc<RingBuffer>,
}

impl Route {
    /// Creates a new route.
    #[must_use]
    pub fn new(source: EndpointId, destination: EndpointId, buffer_size: usize) -> Self {
        Self {
            source,
            destination,
            volume: VolumeControl::new(),
            buffer: Arc::new(RingBuffer::new(buffer_size)),
        }
    }

    /// Gets a reference to the route's buffer.
    #[must_use]
    pub fn buffer(&self) -> &RingBuffer {
        &self.buffer
    }
}

/// The main routing matrix.
pub struct RoutingMatrix {
    /// Routes indexed by destination endpoint.
    routes: RwLock<HashMap<EndpointId, Vec<Arc<Route>>>>,
}

impl RoutingMatrix {
    /// Creates a new empty routing matrix.
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
        }
    }

    /// Adds a route to the matrix.
    pub fn add_route(&self, source: EndpointId, destination: EndpointId, buffer_size: usize) {
        let route = Arc::new(Route::new(source, destination.clone(), buffer_size));
        let mut routes = self.routes.write();
        routes.entry(destination).or_default().push(route);
    }

    /// Removes a route from the matrix.
    ///
    /// # Errors
    ///
    /// Returns an error if the route does not exist.
    pub fn remove_route(&self, source: &EndpointId, destination: &EndpointId) -> Result<()> {
        let mut routes = self.routes.write();
        if let Some(dest_routes) = routes.get_mut(destination) {
            let initial_len = dest_routes.len();
            dest_routes.retain(|r| &r.source != source);
            if dest_routes.len() == initial_len {
                return Err(Error::RouteNotFound(format!("{source} -> {destination}")));
            }
            if dest_routes.is_empty() {
                routes.remove(destination);
            }
            Ok(())
        } else {
            Err(Error::RouteNotFound(format!("{source} -> {destination}")))
        }
    }

    /// Gets all routes feeding a destination.
    #[must_use]
    pub fn routes_to(&self, destination: &EndpointId) -> Vec<Arc<Route>> {
        self.routes
            .read()
            .get(destination)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns the total number of routes.
    #[must_use]
    pub fn route_count(&self) -> usize {
        self.routes.read().values().map(Vec::len).sum()
    }
}

impl Default for RoutingMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_id_display() {
        let ep = EndpointId::new("host1", "Device A", 1);
        assert_eq!(ep.to_string(), "host1:Device A:1");
    }

    #[test]
    fn routing_matrix_add_remove() {
        let matrix = RoutingMatrix::new();
        let src = EndpointId::new("host1", "Out", 1);
        let dst = EndpointId::local("In", 1);

        matrix.add_route(src.clone(), dst.clone(), 1024);
        assert_eq!(matrix.route_count(), 1);
        assert_eq!(matrix.routes_to(&dst).len(), 1);

        matrix.remove_route(&src, &dst).unwrap();
        assert_eq!(matrix.route_count(), 0);
    }

    #[test]
    fn routing_matrix_multiple_sources() {
        let matrix = RoutingMatrix::new();
        let src1 = EndpointId::new("host1", "Out", 1);
        let src2 = EndpointId::new("host2", "Out", 1);
        let dst = EndpointId::local("In", 1);

        matrix.add_route(src1, dst.clone(), 1024);
        matrix.add_route(src2, dst.clone(), 1024);

        assert_eq!(matrix.routes_to(&dst).len(), 2);
    }
}
