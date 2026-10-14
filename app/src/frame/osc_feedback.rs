use opendrop_core::show::Show;
use opendrop_io::command_names::command_id_name;
use opendrop_io::osc_out::OscType;

use crate::AppState;

/// Every value worth mirroring on a controller, as OSC address and value:
/// the crossfader and tempo, each deck's preset, and every command a snapshot
/// can capture (colour, composite and key settings), under the same
/// `/opendrop/<command>` addresses the OSC input listens on.
fn osc_state_values(show: &Show, deck_preset_names: &[String; 4]) -> Vec<(String, OscType)> {
    let mut values = vec![
        ("/opendrop/crossfader".to_string(), OscType::Float(show.crossfader as f32)),
        ("/opendrop/bpm".to_string(), OscType::Float(show.current_bpm() as f32)),
    ];
    for (i, name) in deck_preset_names.iter().enumerate() {
        values.push((format!("/opendrop/deck/{}/preset", i + 1), OscType::String(name.clone())));
    }
    for (id, value) in show.capture_snapshot_values() {
        values.push((format!("/opendrop/{}", command_id_name(id)), OscType::Float(value as f32)));
    }
    values
}

/// Sends whatever changed since the last frame to the OSC output, if it was
/// started from the OSC panel.
pub(crate) fn publish_osc_state(state: &mut AppState) {
    let Some(out) = state.osc_out.as_mut() else { return };
    out.publish(&osc_state_values(&state.show, &state.deck_preset_names));
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendrop_core::commands::CommandId;

    fn names() -> [String; 4] {
        ["A".to_string(), "B".to_string(), String::new(), "D".to_string()]
    }

    fn find<'a>(values: &'a [(String, OscType)], address: &str) -> Option<&'a OscType> {
        values.iter().find(|(a, _)| a == address).map(|(_, v)| v)
    }

    #[test]
    fn the_mix_the_decks_and_the_snapshot_commands_are_published() {
        let mut show = Show::default();
        show.crossfader = 0.75;

        let values = osc_state_values(&show, &names());

        assert_eq!(find(&values, "/opendrop/crossfader"), Some(&OscType::Float(0.75)));
        assert_eq!(find(&values, "/opendrop/deck/1/preset"), Some(&OscType::String("A".to_string())));
        assert_eq!(find(&values, "/opendrop/deck/4/preset"), Some(&OscType::String("D".to_string())));
        let hue = format!("/opendrop/{}", command_id_name(CommandId::ColorHueA));
        assert!(matches!(find(&values, &hue), Some(OscType::Float(_))), "a capturable command is mirrored");
        assert!(find(&values, "/opendrop/bpm").is_some());
    }

    #[test]
    fn addresses_are_unique_so_nothing_overwrites_anything() {
        let values = osc_state_values(&Show::default(), &names());
        let mut addresses: Vec<&str> = values.iter().map(|(a, _)| a.as_str()).collect();
        let before = addresses.len();
        addresses.sort_unstable();
        addresses.dedup();
        assert_eq!(addresses.len(), before);
    }
}
