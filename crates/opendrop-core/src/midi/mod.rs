//! MIDI input handling module
//!
//! Provides MIDI device enumeration, event processing, and mapping to OpenDrop actions.

pub mod mapping;
pub mod persistence;

use midir::{MidiInput, MidiInputConnection};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub use mapping::{
    MidiAction, MidiMapping, MidiMessage, MidiMessageType, TransformCurve, ValueTransform,
};
pub use persistence::{
    create_apc_mini_preset, create_generic_dj_preset, create_launchpad_preset,
    create_nanokontrol2_preset, list_presets, presets_dir, MidiPreset,
};

#[derive(Error, Debug)]
pub enum MidiError {
    #[error("MIDI initialization error: {0}")]
    InitError(String),
    #[error("No MIDI devices found")]
    NoDevices,
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Mapping not found: {0}")]
    MappingNotFound(String),
}

/// Information about a MIDI input port
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiPortInfo {
    /// Port index
    pub index: usize,
    /// Port name
    pub name: String,
}

/// List available MIDI input ports
pub fn list_midi_ports() -> Result<Vec<MidiPortInfo>, MidiError> {
    let midi_in =
        MidiInput::new("OpenDrop").map_err(|e| MidiError::InitError(e.to_string()))?;

    let ports: Vec<MidiPortInfo> = midi_in
        .ports()
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            midi_in.port_name(p).ok().map(|name| MidiPortInfo {
                index: i,
                name,
            })
        })
        .collect();

    Ok(ports)
}

/// Callback type for MIDI events
pub type MidiCallback = Box<dyn Fn(u8, MidiMessage, f32) + Send + 'static>;

/// Callback type for MIDI action events
pub type ActionCallback = Box<dyn Fn(MidiAction, f32) + Send + 'static>;

/// MIDI controller manager
pub struct MidiController {
    /// Currently active connection
    connection: Option<MidiInputConnection<()>>,
    /// Name of the connected port
    connected_port_name: Option<String>,
    /// List of MIDI mappings
    mappings: Arc<Mutex<Vec<MidiMapping>>>,
    /// Callback for processed MIDI actions
    action_callback: Arc<Mutex<Option<ActionCallback>>>,
    /// Learn mode state
    learn_mode: Arc<Mutex<Option<LearnModeState>>>,
}

/// State for MIDI learn mode
#[derive(Debug, Clone)]
pub struct LearnModeState {
    /// The action we're learning a mapping for
    pub target_action: MidiAction,
    /// Name for the new mapping
    pub mapping_name: String,
}

impl MidiController {
    /// Create a new MIDI controller
    pub fn new() -> Self {
        Self {
            connection: None,
            connected_port_name: None,
            mappings: Arc::new(Mutex::new(Vec::new())),
            action_callback: Arc::new(Mutex::new(None)),
            learn_mode: Arc::new(Mutex::new(None)),
        }
    }

    /// Connect to a MIDI input port by index
    pub fn connect(&mut self, port_index: usize) -> Result<(), MidiError> {
        // Disconnect existing connection
        self.disconnect();

        let midi_in =
            MidiInput::new("OpenDrop").map_err(|e| MidiError::InitError(e.to_string()))?;

        let ports = midi_in.ports();
        let port = ports
            .get(port_index)
            .ok_or_else(|| MidiError::DeviceNotFound(format!("Port index {}", port_index)))?;

        let port_name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| "Unknown".to_string());

        // Clone Arcs for the callback closure
        let mappings = Arc::clone(&self.mappings);
        let action_callback = Arc::clone(&self.action_callback);
        let learn_mode = Arc::clone(&self.learn_mode);

        let connection = midi_in
            .connect(
                port,
                "opendrop-midi",
                move |_timestamp, data, _| {
                    let (channel, message) = MidiMessage::parse(data);

                    // Check learn mode first
                    {
                        let mut learn = learn_mode.lock().unwrap();
                        if let Some(state) = learn.take() {
                            // Create new mapping from this MIDI message
                            let midi_type = match message {
                                MidiMessage::NoteOn { note, .. } => {
                                    MidiMessageType::NoteOn { channel, note }
                                }
                                MidiMessage::NoteOff { note, .. } => {
                                    MidiMessageType::NoteOff { channel, note }
                                }
                                MidiMessage::ControlChange { controller, .. } => {
                                    MidiMessageType::ControlChange { channel, controller }
                                }
                                MidiMessage::PitchBend { .. } => {
                                    MidiMessageType::PitchBend { channel }
                                }
                                MidiMessage::ProgramChange { .. } => {
                                    MidiMessageType::ProgramChange { channel }
                                }
                                MidiMessage::Unknown => return,
                            };

                            let new_mapping =
                                MidiMapping::new(state.mapping_name, midi_type, state.target_action);

                            mappings.lock().unwrap().push(new_mapping);
                            tracing::info!("Learned MIDI mapping for {:?}", state.target_action);
                            return;
                        }
                    }

                    // Normal processing: check mappings
                    let mappings_guard = mappings.lock().unwrap();
                    for mapping in mappings_guard.iter() {
                        if mapping.matches(channel, &message) {
                            let value = mapping.transform_value(message.value());

                            if let Some(ref callback) = *action_callback.lock().unwrap() {
                                callback(mapping.action, value);
                            }
                        }
                    }
                },
                (),
            )
            .map_err(|e| MidiError::ConnectionError(e.to_string()))?;

        tracing::info!("Connected to MIDI port: {}", port_name);
        self.connected_port_name = Some(port_name);
        self.connection = Some(connection);
        Ok(())
    }

    /// Disconnect from the current MIDI port
    pub fn disconnect(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.close();
            self.connected_port_name = None;
            tracing::info!("Disconnected from MIDI port");
        }
    }

    /// Get the name of the connected port
    pub fn connected_port_name(&self) -> Option<&str> {
        self.connected_port_name.as_deref()
    }

    /// Check if connected to a MIDI port
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Set the callback for processed MIDI actions
    pub fn set_action_callback<F>(&self, callback: F)
    where
        F: Fn(MidiAction, f32) + Send + 'static,
    {
        *self.action_callback.lock().unwrap() = Some(Box::new(callback));
    }

    /// Add a MIDI mapping
    pub fn add_mapping(&self, mapping: MidiMapping) {
        self.mappings.lock().unwrap().push(mapping);
    }

    /// Remove a MIDI mapping by ID
    pub fn remove_mapping(&self, id: uuid::Uuid) -> bool {
        let mut mappings = self.mappings.lock().unwrap();
        let len_before = mappings.len();
        mappings.retain(|m| m.id != id);
        mappings.len() < len_before
    }

    /// Get all mappings
    pub fn get_mappings(&self) -> Vec<MidiMapping> {
        self.mappings.lock().unwrap().clone()
    }

    /// Clear all mappings
    pub fn clear_mappings(&self) {
        self.mappings.lock().unwrap().clear();
    }

    /// Load mappings from a list
    pub fn load_mappings(&self, mappings: Vec<MidiMapping>) {
        *self.mappings.lock().unwrap() = mappings;
    }

    /// Enter learn mode for a specific action
    pub fn start_learn_mode(&self, action: MidiAction, name: String) {
        *self.learn_mode.lock().unwrap() = Some(LearnModeState {
            target_action: action,
            mapping_name: name,
        });
        tracing::info!("Started MIDI learn mode for {:?}", action);
    }

    /// Cancel learn mode
    pub fn cancel_learn_mode(&self) {
        *self.learn_mode.lock().unwrap() = None;
        tracing::info!("Cancelled MIDI learn mode");
    }

    /// Check if in learn mode
    pub fn is_learning(&self) -> bool {
        self.learn_mode.lock().unwrap().is_some()
    }
}

impl Default for MidiController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MidiController {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_ports_no_panic() {
        // This test just ensures the function doesn't crash
        // It may return an empty list if no MIDI devices are connected
        let result = list_midi_ports();
        assert!(result.is_ok());
    }

    #[test]
    fn test_midi_controller_new() {
        let controller = MidiController::new();
        assert!(!controller.is_connected());
        assert!(!controller.is_learning());
        assert!(controller.get_mappings().is_empty());
    }

    #[test]
    fn test_add_remove_mapping() {
        let controller = MidiController::new();

        let mapping = MidiMapping::new(
            "Test",
            MidiMessageType::ControlChange {
                channel: 0,
                controller: 1,
            },
            MidiAction::CrossfaderPosition,
        );
        let id = mapping.id;

        controller.add_mapping(mapping);
        assert_eq!(controller.get_mappings().len(), 1);

        assert!(controller.remove_mapping(id));
        assert!(controller.get_mappings().is_empty());
    }

    #[test]
    fn test_learn_mode() {
        let controller = MidiController::new();

        assert!(!controller.is_learning());

        controller.start_learn_mode(MidiAction::DeckVolume(0), "Deck 1 Volume".to_string());
        assert!(controller.is_learning());

        controller.cancel_learn_mode();
        assert!(!controller.is_learning());
    }

    #[test]
    fn test_load_mappings() {
        let controller = MidiController::new();

        let mappings = vec![
            MidiMapping::new(
                "Crossfader",
                MidiMessageType::ControlChange {
                    channel: 0,
                    controller: 1,
                },
                MidiAction::CrossfaderPosition,
            ),
            MidiMapping::new(
                "Deck 1 Volume",
                MidiMessageType::ControlChange {
                    channel: 0,
                    controller: 7,
                },
                MidiAction::DeckVolume(0),
            ),
        ];

        controller.load_mappings(mappings);
        assert_eq!(controller.get_mappings().len(), 2);

        controller.clear_mappings();
        assert!(controller.get_mappings().is_empty());
    }
}
