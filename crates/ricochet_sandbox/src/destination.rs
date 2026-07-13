#[allow(dead_code)]
pub struct DestinationGrant(String);

#[cfg(test)]
mod tests {
    use super::DestinationGrant;
    use crate::DiagnosticMetadata;

    #[test]
    fn diagnostic_metadata_debug_redacts_raw_destination() {
        let raw_destination = "private.internal.example:443";
        let metadata = DiagnosticMetadata::empty()
            .with_destination(DestinationGrant(raw_destination.to_owned()));

        let debug = format!("{metadata:?}");

        assert!(
            !debug.contains(raw_destination),
            "debug output leaked raw destination: {debug}"
        );
    }
}
