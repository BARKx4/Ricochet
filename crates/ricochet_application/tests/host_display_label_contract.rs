use ricochet_application::HostDisplayLabel;

#[test]
fn host_display_label_accepts_exact_utf8_byte_boundaries() {
    for valid in ["a".to_string(), "a".repeat(160), "é".repeat(80)] {
        let label = HostDisplayLabel::parse(&valid).expect("valid host display label");
        assert_eq!(label.as_str(), valid);
    }
}

#[test]
fn host_display_label_rejects_empty_overlength_and_all_control_families() {
    let mut invalid = vec![String::new(), "a".repeat(161), "é".repeat(81)];
    invalid.extend(
        (0_u32..=0x1f)
            .filter_map(char::from_u32)
            .map(|control| format!("before{control}after")),
    );
    invalid.extend(
        (0x7f_u32..=0x9f)
            .filter_map(char::from_u32)
            .map(|control| format!("before{control}after")),
    );
    invalid.extend(
        [
            '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
            '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ]
        .into_iter()
        .map(|control| format!("before{control}after")),
    );

    for value in invalid {
        assert!(
            HostDisplayLabel::parse(&value).is_err(),
            "invalid label was accepted: {value:?}"
        );
    }
}
