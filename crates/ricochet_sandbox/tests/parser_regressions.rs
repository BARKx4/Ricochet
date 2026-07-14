use std::panic::{catch_unwind, AssertUnwindSafe, UnwindSafe};

use ricochet_sandbox::{
    AuthenticatedCodec, BrokerRequest, CatalogSnapshot, DestinationGrant, EndpointRole,
    ExecutionPolicyRequest, ProtocolKey, ProtocolMessage, RequestId, MAX_FRAME_BYTES,
};

fn assert_no_panic<T>(label: &str, operation: impl FnOnce() -> T + UnwindSafe) -> T {
    match catch_unwind(operation) {
        Ok(result) => result,
        Err(_) => panic!("parser panicked for fixed corpus entry {label}"),
    }
}

#[test]
fn destination_parser_fixed_malformed_and_boundary_corpus_never_panics() {
    let corpus = [
        "",
        ":",
        "example.com",
        "example.com:0",
        "example.com:65536",
        "example.com:-1",
        "example.com:443:extra",
        "127.0.0.1:443",
        "[::1]:443",
        "localhost:443",
        "white space.example:443",
        "a..example:443",
        "-bad.example:443",
        "bad-.example:443",
        "example.com:65535",
        "BÜCHER.example.:443",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example:443",
    ];

    for input in corpus {
        assert_no_panic(input, || {
            let _ = DestinationGrant::parse(input);
        });
    }
}

#[test]
fn catalog_json_fixed_malformed_and_boundary_corpus_never_panics() {
    let corpus: &[&[u8]] = &[
        b"",
        b"{",
        b"null",
        b"[]",
        b"{}",
        b"{\"goblin\":true}",
        b"{\"schema_version\":\"1\"}",
        br#"{"schema_version":1,"generation":0,"platform":{"os":"windows","arch":"x86_64"},"records":[],"revoked_tools":[]}"#,
        br#"{"schema_version":1,"generation":7,"platform":{"os":"windows","arch":"x86_64"},"records":[],"revoked_tools":[]}"#,
        br#"{"schema_version":1,"schema_version":1,"generation":7,"platform":{"os":"windows","arch":"x86_64"},"records":[],"revoked_tools":[]}"#,
        &[0xff, 0xfe, 0xfd],
    ];

    for (index, input) in corpus.iter().enumerate() {
        let _ = assert_no_panic(&format!("catalog-{index}"), || {
            serde_json::from_slice::<CatalogSnapshot>(input)
        });
    }
}

#[test]
fn policy_json_fixed_malformed_and_boundary_corpus_never_panics() {
    let corpus: &[&[u8]] = &[
        b"",
        b"{",
        b"null",
        b"[]",
        b"{}",
        b"{\"goblin\":true}",
        br#"{"schema_version":1,"access":"unknown","allow_process":true,"allow_pty":true,"workspace":null,"scratch_disposition":"delete_on_clean_close_retain_otherwise","catalog_generation":7,"activated_tools":[],"destinations":[],"environment":{"base":[]},"resource_limits":null,"audit_policy":{"arguments":"count_only"}}"#,
        br#"{"schema_version":1,"access":"full","allow_process":true,"allow_pty":true,"workspace":null,"scratch_disposition":"delete_on_clean_close_retain_otherwise","catalog_generation":7,"activated_tools":[],"destinations":[],"environment":{"base":[]},"resource_limits":null,"audit_policy":{"arguments":"count_only"}}"#,
        br#"{"schema_version":1,"access":"read","allow_process":true,"allow_pty":true,"workspace":{"requested_root":"C:/workspace"},"scratch_disposition":"delete_on_clean_close_retain_otherwise","catalog_generation":7,"activated_tools":[],"destinations":[],"environment":{"base":[]},"resource_limits":{"descendant_processes":4294967295,"memory_bytes":18446744073709551615,"cpu_time_ms":18446744073709551615,"wall_time_ms":18446744073709551615,"open_descriptors_or_handles":4294967295,"captured_output_bytes":18446744073709551615},"audit_policy":{"arguments":"count_only"}}"#,
        br#"{"schema_version":1,"access":"full","allow_process":true,"allow_pty":true,"scratch_disposition":"delete_on_clean_close_retain_otherwise","catalog_generation":7,"activated_tools":[],"destinations":[],"environment":{"base":[]},"resource_limits":null,"audit_policy":{"arguments":"count_only"}}"#,
        &[0xff, 0x00, b'{'],
    ];

    for (index, input) in corpus.iter().enumerate() {
        let _ = assert_no_panic(&format!("policy-{index}"), || {
            serde_json::from_slice::<ExecutionPolicyRequest>(input)
        });
    }
}

#[test]
fn protocol_message_json_fixed_malformed_and_boundary_corpus_never_panics() {
    let corpus: &[&[u8]] = &[
        b"",
        b"{",
        b"null",
        b"[]",
        b"{}",
        b"{\"type\":\"unknown\"}",
        br#"{"type":"request","body":{"request_id":1,"request":{"type":"ping"}}}"#,
        br#"{"type":"request","body":{"request_id":18446744073709551615,"request":{"type":"ping"}}}"#,
        br#"{"type":"request","body":{"request_id":1,"request":{"type":"ping","goblin":true}}}"#,
        br#"{"type":"request","body":{"request_id":1,"request":{"type":"process_write","body":{"session_id":"session-01","process_id":0,"bytes":"%%%","close_stdin":false}}}}"#,
        &[0xff, 0xfe],
    ];

    for (index, input) in corpus.iter().enumerate() {
        let _ = assert_no_panic(&format!("message-{index}"), || {
            serde_json::from_slice::<ProtocolMessage>(input)
        });
    }
}

fn broker_codec() -> AuthenticatedCodec {
    AuthenticatedCodec::new(
        EndpointRole::Broker,
        ProtocolKey::from_bytes([9; 32]),
        ProtocolKey::from_bytes([7; 32]),
    )
}

fn valid_ping_frame() -> Vec<u8> {
    let mut host = AuthenticatedCodec::new(
        EndpointRole::Host,
        ProtocolKey::from_bytes([7; 32]),
        ProtocolKey::from_bytes([9; 32]),
    );
    host.encode(ProtocolMessage::request(
        RequestId::new(1),
        BrokerRequest::Ping,
    ))
    .unwrap()
}

#[test]
fn frame_decoder_fixed_malformed_and_boundary_corpus_never_panics() {
    let valid = valid_ping_frame();
    let declared_one = 1_u32.to_be_bytes().to_vec();
    let declared_max = (MAX_FRAME_BYTES as u32).to_be_bytes().to_vec();
    let declared_oversized = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec();
    let declared_untrusted_max = u32::MAX.to_be_bytes().to_vec();
    let mut trailing = valid.clone();
    trailing.push(0);
    let corpus = vec![
        Vec::new(),
        vec![0],
        vec![0, 0],
        vec![0, 0, 0],
        0_u32.to_be_bytes().to_vec(),
        declared_one,
        declared_max,
        declared_oversized,
        declared_untrusted_max,
        vec![0, 0, 0, 1, b'{'],
        trailing,
        valid,
    ];

    for (index, input) in corpus.iter().enumerate() {
        let mut codec = broker_codec();
        match catch_unwind(AssertUnwindSafe(|| {
            let _ = codec.decode(input);
        })) {
            Ok(()) => {}
            Err(_) => panic!("frame decoder panicked for fixed corpus entry {index}"),
        }
    }
}

#[test]
fn oversized_declared_frame_lengths_are_rejected_from_prefix_only() {
    for declared in [(MAX_FRAME_BYTES + 1) as u32, u32::MAX] {
        let prefix = declared.to_be_bytes();
        let mut codec = broker_codec();
        let rejected = match catch_unwind(AssertUnwindSafe(|| codec.decode(&prefix).is_err())) {
            Ok(rejected) => rejected,
            Err(_) => panic!("decoder attempted to act on untrusted declared length {declared}"),
        };

        assert!(rejected);
        assert_eq!(prefix.len(), 4);
    }
}
