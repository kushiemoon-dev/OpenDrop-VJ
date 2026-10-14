//! Startup failures that cannot be recovered from. A GUI user launching the
//! app from a file manager never sees stderr, so these also go through a
//! native message box before the process exits.

use std::any::Any;

const DIALOG_TITLE: &str = "OpenDrop VJ";

/// Turns a caught panic payload into a readable message. `panic!("..")` and
/// `.expect("..")` carry a `&str` or a `String`; anything else is opaque.
pub(crate) fn describe_panic(payload: &(dyn Any + Send)) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        (*msg).to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "unexpected internal error".to_string()
    }
}

/// Logs `detail` to stderr and shows it to the user. Blocks until the dialog
/// is dismissed; if no dialog backend is available the stderr line remains.
pub(crate) fn report_fatal(summary: &str, detail: &str) {
    eprintln!("[app] {summary}: {detail}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(DIALOG_TITLE)
        .set_description(format!("{summary}.\n\n{detail}"))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_str_and_string_panics() {
        let from_str = std::panic::catch_unwind(|| panic!("static message")).unwrap_err();
        assert_eq!(describe_panic(from_str.as_ref()), "static message");

        let n = 3;
        let from_string = std::panic::catch_unwind(|| panic!("formatted {n}")).unwrap_err();
        assert_eq!(describe_panic(from_string.as_ref()), "formatted 3");
    }

    #[test]
    fn describes_opaque_payloads_generically() {
        let opaque = std::panic::catch_unwind(|| std::panic::panic_any(42_u8)).unwrap_err();
        assert_eq!(describe_panic(opaque.as_ref()), "unexpected internal error");
    }
}
