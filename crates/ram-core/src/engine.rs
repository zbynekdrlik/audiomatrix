//! Audio routing engine that manages connections and destinations.
//!
//! The `AudioEngine` is the central coordinator for audio routing. It manages:
//! - Source connections (inputs to the routing matrix)
//! - Destination channels (outputs with N:1 mixing)
//! - Connection lifecycle (create, update, delete)

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::connection::{ConnectionId, SourceConnection};
use crate::destination::{DestinationChannel, HeadroomMode};
use crate::{Error, Result, DEFAULT_BUFFER_SIZE};

/// Configuration for the audio engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Default buffer size for connections.
    pub buffer_size: usize,
    /// Default headroom mode for destinations.
    pub default_headroom_mode: HeadroomMode,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            default_headroom_mode: HeadroomMode::Clip,
        }
    }
}

/// The main audio routing engine.
///
/// This manages all connections and destinations in the system. It provides
/// a high-level API for creating, modifying, and removing audio routes.
///
/// # Thread Safety
///
/// The engine uses `RwLock` for configuration changes but provides lock-free
/// access to connection parameters (gain, mute, etc.) for real-time safety.
pub struct AudioEngine {
    /// Engine configuration.
    config: EngineConfig,
    /// All connections indexed by their ID.
    connections: RwLock<HashMap<ConnectionId, Arc<SourceConnection>>>,
    /// Destination channels indexed by destination ID string.
    destinations: RwLock<HashMap<String, Arc<DestinationChannel>>>,
}

impl AudioEngine {
    /// Creates a new audio engine with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(EngineConfig::default())
    }

    /// Creates a new audio engine with the specified configuration.
    #[must_use]
    pub fn with_config(config: EngineConfig) -> Self {
        Self {
            config,
            connections: RwLock::new(HashMap::new()),
            destinations: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new connection.
    ///
    /// If the destination doesn't exist, it will be created automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if a connection with the same ID already exists.
    pub fn create_connection(&self, id: ConnectionId) -> Result<Arc<SourceConnection>> {
        let mut connections = self.connections.write();

        if connections.contains_key(&id) {
            return Err(Error::ConnectionExists(id.to_string()));
        }

        // Create the connection
        let connection = Arc::new(SourceConnection::new(id.clone(), self.config.buffer_size));

        // Ensure destination exists
        let dest_id = id.destination_id();
        self.ensure_destination(&dest_id);

        // Add connection to destination
        if let Some(dest) = self.destinations.read().get(&dest_id) {
            dest.add_source(Arc::clone(&connection));
        }

        connections.insert(id, Arc::clone(&connection));
        Ok(connection)
    }

    /// Gets an existing connection.
    #[must_use]
    pub fn get_connection(&self, id: &ConnectionId) -> Option<Arc<SourceConnection>> {
        self.connections.read().get(id).cloned()
    }

    /// Removes a connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection doesn't exist.
    pub fn remove_connection(&self, id: &ConnectionId) -> Result<()> {
        let mut connections = self.connections.write();

        if connections.remove(id).is_none() {
            return Err(Error::ConnectionNotFound(id.to_string()));
        }

        // Remove from destination
        let dest_id = id.destination_id();
        if let Some(dest) = self.destinations.read().get(&dest_id) {
            dest.remove_source(id);
        }

        Ok(())
    }

    /// Returns the total number of connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.read().len()
    }

    /// Gets a destination channel.
    #[must_use]
    pub fn get_destination(&self, id: &str) -> Option<Arc<DestinationChannel>> {
        self.destinations.read().get(id).cloned()
    }

    /// Returns the number of destination channels.
    #[must_use]
    pub fn destination_count(&self) -> usize {
        self.destinations.read().len()
    }

    /// Sets the gain for a connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection doesn't exist.
    pub fn set_connection_gain(&self, id: &ConnectionId, gain: f32) -> Result<()> {
        let connections = self.connections.read();
        let conn = connections
            .get(id)
            .ok_or_else(|| Error::ConnectionNotFound(id.to_string()))?;
        conn.set_gain(gain);
        Ok(())
    }

    /// Sets the mute state for a connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection doesn't exist.
    pub fn set_connection_muted(&self, id: &ConnectionId, muted: bool) -> Result<()> {
        let connections = self.connections.read();
        let conn = connections
            .get(id)
            .ok_or_else(|| Error::ConnectionNotFound(id.to_string()))?;
        conn.set_muted(muted);
        Ok(())
    }

    /// Sets the enabled state for a connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection doesn't exist.
    pub fn set_connection_enabled(&self, id: &ConnectionId, enabled: bool) -> Result<()> {
        let connections = self.connections.read();
        let conn = connections
            .get(id)
            .ok_or_else(|| Error::ConnectionNotFound(id.to_string()))?;
        conn.set_enabled(enabled);
        Ok(())
    }

    /// Sets the headroom mode for a destination.
    ///
    /// # Errors
    ///
    /// Returns an error if the destination doesn't exist.
    pub fn set_destination_headroom_mode(&self, dest_id: &str, mode: HeadroomMode) -> Result<()> {
        let destinations = self.destinations.read();
        let dest = destinations
            .get(dest_id)
            .ok_or_else(|| Error::DestinationNotFound(dest_id.to_string()))?;
        dest.set_headroom_mode(mode);
        Ok(())
    }

    /// Batch creates multiple connections.
    ///
    /// Returns the results for each connection. Partial success is allowed.
    pub fn batch_create_connections(
        &self,
        ids: Vec<ConnectionId>,
    ) -> Vec<Result<Arc<SourceConnection>>> {
        ids.into_iter()
            .map(|id| self.create_connection(id))
            .collect()
    }

    /// Batch removes multiple connections.
    ///
    /// Returns the results for each removal. Partial success is allowed.
    pub fn batch_remove_connections(&self, ids: &[ConnectionId]) -> Vec<Result<()>> {
        ids.iter().map(|id| self.remove_connection(id)).collect()
    }

    /// Batch updates gain for multiple connections.
    ///
    /// If `relative` is true, the gain is applied as a multiplier to
    /// each connection's current gain.
    pub fn batch_set_gain(
        &self,
        ids: &[ConnectionId],
        gain: f32,
        relative: bool,
    ) -> Vec<Result<()>> {
        ids.iter()
            .map(|id| {
                let connections = self.connections.read();
                let conn = connections
                    .get(id)
                    .ok_or_else(|| Error::ConnectionNotFound(id.to_string()))?;
                if relative {
                    conn.set_gain(conn.gain() * gain);
                } else {
                    conn.set_gain(gain);
                }
                Ok(())
            })
            .collect()
    }

    fn ensure_destination(&self, dest_id: &str) {
        let mut destinations = self.destinations.write();
        if !destinations.contains_key(dest_id) {
            let dest = Arc::new(DestinationChannel::new(dest_id, self.config.buffer_size));
            dest.set_headroom_mode(self.config.default_headroom_mode);
            destinations.insert(dest_id.to_string(), dest);
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id() -> ConnectionId {
        ConnectionId::local("Mic", 1, "Speaker", 1)
    }

    fn test_id_2() -> ConnectionId {
        ConnectionId::local("Mic", 2, "Speaker", 1)
    }

    #[test]
    fn engine_new() {
        let engine = AudioEngine::new();
        assert_eq!(engine.connection_count(), 0);
        assert_eq!(engine.destination_count(), 0);
    }

    #[test]
    fn engine_with_config() {
        let config = EngineConfig {
            buffer_size: 512,
            default_headroom_mode: HeadroomMode::AutoGain,
        };
        let engine = AudioEngine::with_config(config);
        assert_eq!(engine.config.buffer_size, 512);
    }

    #[test]
    fn create_connection() {
        let engine = AudioEngine::new();
        let id = test_id();

        let conn = engine.create_connection(id.clone()).unwrap();
        assert_eq!(conn.id(), &id);
        assert_eq!(engine.connection_count(), 1);
        assert_eq!(engine.destination_count(), 1);
    }

    #[test]
    fn create_duplicate_connection_fails() {
        let engine = AudioEngine::new();
        let id = test_id();

        engine.create_connection(id.clone()).unwrap();
        let result = engine.create_connection(id);
        assert!(result.is_err());
    }

    #[test]
    fn get_connection() {
        let engine = AudioEngine::new();
        let id = test_id();

        engine.create_connection(id.clone()).unwrap();
        assert!(engine.get_connection(&id).is_some());

        let other_id = test_id_2();
        assert!(engine.get_connection(&other_id).is_none());
    }

    #[test]
    fn remove_connection() {
        let engine = AudioEngine::new();
        let id = test_id();

        engine.create_connection(id.clone()).unwrap();
        assert_eq!(engine.connection_count(), 1);

        engine.remove_connection(&id).unwrap();
        assert_eq!(engine.connection_count(), 0);
    }

    #[test]
    fn remove_nonexistent_connection_fails() {
        let engine = AudioEngine::new();
        let id = test_id();

        let result = engine.remove_connection(&id);
        assert!(result.is_err());
    }

    #[test]
    fn set_connection_gain() {
        let engine = AudioEngine::new();
        let id = test_id();

        let conn = engine.create_connection(id.clone()).unwrap();
        engine.set_connection_gain(&id, 0.5).unwrap();
        assert!((conn.gain() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn set_connection_muted() {
        let engine = AudioEngine::new();
        let id = test_id();

        let conn = engine.create_connection(id.clone()).unwrap();
        engine.set_connection_muted(&id, true).unwrap();
        assert!(conn.is_muted());
    }

    #[test]
    fn set_connection_enabled() {
        let engine = AudioEngine::new();
        let id = test_id();

        let conn = engine.create_connection(id.clone()).unwrap();
        engine.set_connection_enabled(&id, false).unwrap();
        assert!(!conn.is_enabled());
    }

    #[test]
    fn set_destination_headroom_mode() {
        let engine = AudioEngine::new();
        let id = test_id();

        engine.create_connection(id.clone()).unwrap();
        let dest_id = id.destination_id();

        engine
            .set_destination_headroom_mode(&dest_id, HeadroomMode::AutoGain)
            .unwrap();

        let dest = engine.get_destination(&dest_id).unwrap();
        assert_eq!(dest.headroom_mode(), HeadroomMode::AutoGain);
    }

    #[test]
    fn multiple_connections_same_destination() {
        let engine = AudioEngine::new();
        let id1 = ConnectionId::local("Mic", 1, "Speaker", 1);
        let id2 = ConnectionId::local("Mic", 2, "Speaker", 1);

        engine.create_connection(id1.clone()).unwrap();
        engine.create_connection(id2.clone()).unwrap();

        assert_eq!(engine.connection_count(), 2);
        assert_eq!(engine.destination_count(), 1);

        let dest = engine.get_destination(&id1.destination_id()).unwrap();
        assert_eq!(dest.source_count(), 2);
    }

    #[test]
    fn batch_create_connections() {
        let engine = AudioEngine::new();
        let ids = vec![
            ConnectionId::local("Mic", 1, "Out", 1),
            ConnectionId::local("Mic", 2, "Out", 1),
            ConnectionId::local("Mic", 3, "Out", 2),
        ];

        let results = engine.batch_create_connections(ids);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(Result::is_ok));
        assert_eq!(engine.connection_count(), 3);
    }

    #[test]
    fn batch_remove_connections() {
        let engine = AudioEngine::new();
        let id1 = ConnectionId::local("Mic", 1, "Out", 1);
        let id2 = ConnectionId::local("Mic", 2, "Out", 1);

        engine.create_connection(id1.clone()).unwrap();
        engine.create_connection(id2.clone()).unwrap();

        let results = engine.batch_remove_connections(&[id1, id2]);
        assert!(results.iter().all(Result::is_ok));
        assert_eq!(engine.connection_count(), 0);
    }

    #[test]
    fn batch_set_gain_absolute() {
        let engine = AudioEngine::new();
        let id1 = ConnectionId::local("Mic", 1, "Out", 1);
        let id2 = ConnectionId::local("Mic", 2, "Out", 1);

        let conn1 = engine.create_connection(id1.clone()).unwrap();
        let conn2 = engine.create_connection(id2.clone()).unwrap();

        engine.batch_set_gain(&[id1, id2], 0.5, false);

        assert!((conn1.gain() - 0.5).abs() < f32::EPSILON);
        assert!((conn2.gain() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn batch_set_gain_relative() {
        let engine = AudioEngine::new();
        let id1 = ConnectionId::local("Mic", 1, "Out", 1);
        let id2 = ConnectionId::local("Mic", 2, "Out", 1);

        let conn1 = engine.create_connection(id1.clone()).unwrap();
        let conn2 = engine.create_connection(id2.clone()).unwrap();

        conn1.set_gain(0.8);
        conn2.set_gain(0.6);

        engine.batch_set_gain(&[id1, id2], 0.5, true);

        assert!((conn1.gain() - 0.4).abs() < f32::EPSILON);
        assert!((conn2.gain() - 0.3).abs() < f32::EPSILON);
    }
}
