use ricochet_sandbox::DestinationGrant;
use std::cmp::Ordering;
use std::hash::{DefaultHasher, Hash, Hasher};

#[test]
fn destination_is_an_exact_normalized_host_and_port() {
    let grant = DestinationGrant::parse("BÜCHER.example.:443").unwrap();
    assert_eq!(grant.host(), "xn--bcher-kva.example");
    assert_eq!(grant.port(), 443);
    assert_eq!(grant.to_string(), "xn--bcher-kva.example:443");
}

#[test]
fn ambiguous_or_address_based_destinations_fail() {
    for denied in [
        "example.com",
        "https://example.com:443",
        "user@example.com:443",
        "*.example.com:443",
        "127.0.0.1:443",
        "[::1]:443",
        "localhost:443",
        "metadata.google.internal:0",
    ] {
        assert!(
            DestinationGrant::parse(denied).is_err(),
            "accepted {denied}"
        );
    }
}

#[test]
fn equivalent_inputs_compare_and_hash_by_canonical_destination() {
    let unicode = DestinationGrant::parse("BÜCHER.Example.:443").unwrap();
    let canonical = DestinationGrant::new("xn--bcher-kva.example", 443).unwrap();

    assert!(unicode == canonical);
    assert_eq!(unicode.cmp(&canonical), Ordering::Equal);

    let mut unicode_hash = DefaultHasher::new();
    unicode.hash(&mut unicode_hash);
    let mut canonical_hash = DefaultHasher::new();
    canonical.hash(&mut canonical_hash);
    assert_eq!(unicode_hash.finish(), canonical_hash.finish());
}

#[test]
fn strict_idna_deviation_and_disallowed_characters_fail() {
    for denied in [
        "faß.de:443",
        "ẞ.de:443",
        "ς.example:443",
        "xn--fa-hia.de:443",
        "a\u{200c}b.example:443",
        "a\u{200d}b.example:443",
        "\u{fffd}.example:443",
    ] {
        assert!(
            DestinationGrant::parse(denied).is_err(),
            "accepted {denied:?}"
        );
    }
}

#[test]
fn host_syntax_and_dns_bounds_are_enforced_after_normalization() {
    let long_label = format!("{}.example:443", "a".repeat(64));
    let long_host = format!("{}:443", vec!["a".repeat(63); 4].join("."));

    for denied in [
        ":443",
        ".example:443",
        "example..com:443",
        "example.com..:443",
        "-example.com:443",
        "example-.com:443",
        "example.com/path:443",
        "example.com?query:443",
        "example.com#fragment:443",
        "example.com%25eth0:443",
        "example.com:443:80",
        "example.com: 443",
        " example.com:443",
        "example.com:443 ",
        "[2001:db8::1]:443",
        "service.localhost:443",
        long_label.as_str(),
        long_host.as_str(),
    ] {
        assert!(
            DestinationGrant::parse(denied).is_err(),
            "accepted {denied:?}"
        );
    }

    assert!(DestinationGrant::new("example.com", 0).is_err());
    assert!(DestinationGrant::new("127.0.0.1", 443).is_err());
    assert!(DestinationGrant::new("example.com/path", 443).is_err());
}

#[test]
fn serde_uses_the_canonical_string_and_revalidates_input() {
    let grant = DestinationGrant::parse("BÜCHER.Example.:443").unwrap();
    assert_eq!(
        serde_json::to_string(&grant).unwrap(),
        "\"xn--bcher-kva.example:443\""
    );

    let decoded = serde_json::from_str::<DestinationGrant>("\"BÜCHER.Example.:443\"").unwrap();
    assert_eq!(decoded.host(), "xn--bcher-kva.example");
    assert_eq!(decoded.port(), 443);

    for denied in [
        "\"example.com\"",
        "\"127.0.0.1:443\"",
        "\"faß.de:443\"",
        "{\"host\":\"example.com\",\"port\":443}",
    ] {
        assert!(
            serde_json::from_str::<DestinationGrant>(denied).is_err(),
            "deserialized {denied}"
        );
    }
}
