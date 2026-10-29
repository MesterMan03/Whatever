use super::{
    codec,
    registry::{NetworkRegistry, NetworkRequest},
    schema::{ClientDataDefinition, Schema, Specification},
};
use crate::runtime::RuntimeMode;
use serde_json::{Value, json};
fn specification() -> Specification {
    serde_json::from_value(json!({"id":"fixture:game","states":[
        {"name":"login","serverbound":[{"name":"hello","fields":[{"name":"name","schema":{"kind":"string","max_length":64}}]}],"clientbound":[{"name":"welcome","fields":[{"name":"id","schema":{"kind":"i32","min":0}}]}]},
        {"name":"play","serverbound":[{"name":"value","fields":[{"name":"n","schema":{"kind":"varint"}}]}],"clientbound":[]}
    ]})).expect("fixture schema")
}
fn request(id: &str, packet: &str, data: Value) -> NetworkRequest {
    NetworkRequest::Encode {
        registration_id: id.into(),
        packet: packet.into(),
        data,
    }
}
#[test]
fn packet_ids_cross_states_and_directions_and_schema_order_is_authoritative() {
    let spec = specification();
    let compiled = spec.compile("fixture").expect("compile");
    assert_eq!(
        compiled
            .packets
            .iter()
            .map(|p| p.info.id)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(compiled.packets[2].info.packet, "play.value");
    assert_eq!(
        compiled.fingerprint,
        spec.compile("fixture").expect("repeat").fingerprint
    );
    assert!(spec.compile("another_mod").is_err());
    let mut invalid = spec.clone();
    invalid.states[0].serverbound[0].fields[0].name = "__proto__".into();
    assert!(invalid.compile("fixture").is_err());
    let mut invalid = spec;
    let duplicate = invalid.states[0].serverbound[0].clone();
    invalid.states[0].serverbound.push(duplicate);
    assert!(invalid.compile("fixture").is_err());
}
#[test]
fn codec_uses_big_endian_fixed_numbers_and_unsigned_lengths_with_zigzag_values() {
    let spec:Specification=serde_json::from_value(json!({"id":"fixture:golden","states":[{"name":"s","serverbound":[{"name":"p","fields":[
        {"name":"integer","schema":{"kind":"i32"}}, {"name":"float","schema":{"kind":"f64"}},
        {"name":"variable","schema":{"kind":"varint"}}, {"name":"flag","schema":{"kind":"bool"}},
        {"name":"text","schema":{"kind":"string"}}, {"name":"values","schema":{"kind":"array","element":{"kind":"varint"}}},
        {"name":"maybe","schema":{"kind":"optional","inner":{"kind":"bool"}}}
    ]}],"clientbound":[]}]})).expect("golden schema");
    let compiled = spec.compile("fixture").expect("compile");
    let value = json!({"integer":-123,"float":1.5,"variable":-1,"flag":true,"text":"é","values":[0,-1,300],"maybe":null});
    let bytes = codec::encode(&compiled.packets[0], &value).expect("encode");
    let mut expected = vec![0, 0];
    expected.extend_from_slice(&(-123i32).to_be_bytes());
    expected.extend_from_slice(&1.5f64.to_be_bytes());
    expected.extend_from_slice(&[1, 1, 2, 0xc3, 0xa9, 3, 0, 1, 0xd8, 4, 0]);
    assert_eq!(bytes, expected);
    assert_eq!(
        codec::decode(&compiled.packets, &bytes).expect("decode")["data"],
        value
    );
    let mut nonzero = bytes.clone();
    nonzero[15] = 7;
    assert_eq!(
        codec::decode(&compiled.packets, &nonzero).expect("nonzero bool")["data"]["flag"],
        true
    );
}
#[test]
fn zigzag_i32_boundaries_and_malformed_packets_are_checked() {
    let compiled = specification().compile("fixture").expect("compile");
    for n in [i32::MIN, -1, 0, 1, i32::MAX] {
        let bytes = codec::encode(&compiled.packets[2], &json!({"n":n})).expect("encode");
        assert!(bytes.len() <= 7);
        assert_eq!(
            codec::decode(&compiled.packets, &bytes).expect("decode")["data"]["n"],
            n
        );
    }
    for bytes in [
        vec![],
        vec![0],
        vec![255, 255],
        vec![0, 0, 1, 255],
        vec![0, 0, 0, 9],
        vec![2, 0],
        vec![0, 2, 0x80, 0],
        vec![0, 2, 255, 255, 255, 255, 16],
    ] {
        assert!(
            codec::decode(&compiled.packets, &bytes).is_err(),
            "accepted malformed packet {bytes:?}"
        );
    }
    assert!(codec::encode(&compiled.packets[1], &json!({"id":-1})).is_err());
    assert!(codec::encode(&compiled.packets[0], &json!({"name":"ok","unknown":1})).is_err());
}
#[test]
fn nested_optional_struct_arrays_roundtrip_with_bounds() {
    let mut spec = specification();
    spec.states[0].serverbound[0].fields[0].schema=serde_json::from_value(json!({"kind":"array","max_length":2,"element":{"kind":"optional","inner":{"kind":"struct","fields":[{"name":"x","schema":{"kind":"f64","min":-1,"max":1}}]}}})).expect("nested schema");
    let compiled = spec.compile("fixture").expect("compile");
    let data = json!({"name":[null,{"x":0.5}]});
    let bytes = codec::encode(&compiled.packets[0], &data).expect("encode");
    assert_eq!(
        codec::decode(&compiled.packets, &bytes).expect("decode")["data"],
        data
    );
    assert!(codec::encode(&compiled.packets[0], &json!({"name":[{"x":2}]})).is_err());
    assert!(codec::decode(&compiled.packets, &[0, 0, 3]).is_err());
}
fn client_data() -> ClientDataDefinition {
    ClientDataDefinition {schema:serde_json::from_value(json!({"kind":"struct","fields":[{"name":"tags","schema":{"kind":"array","element":{"kind":"string"}}}]})).expect("client schema"),initial:json!({"tags":[]})}
}
#[test]
fn native_registration_enforces_role_owner_direction_and_lifetime() {
    let mut registry = NetworkRegistry::default();
    assert!(
        registry
            .request(
                "fixture",
                RuntimeMode::Server,
                NetworkRequest::Register {
                    specification: specification(),
                    side: RuntimeMode::Client,
                    client_data: None
                }
            )
            .is_err()
    );
    let registered = registry
        .request(
            "fixture",
            RuntimeMode::Client,
            NetworkRequest::Register {
                specification: specification(),
                side: RuntimeMode::Client,
                client_data: None,
            },
        )
        .expect("register");
    let id = registered["registration_id"].as_str().expect("handle");
    assert!(
        registry
            .request(
                "fixture",
                RuntimeMode::Client,
                request(id, "login.hello", json!({"name":"Alex"}))
            )
            .is_ok()
    );
    assert!(
        registry
            .request(
                "fixture",
                RuntimeMode::Client,
                request(id, "login.welcome", json!({"id":1}))
            )
            .is_err()
    );
    assert!(
        registry
            .request(
                "other",
                RuntimeMode::Client,
                request(id, "login.hello", json!({"name":"Alex"}))
            )
            .is_err()
    );
    let error = registry
        .request(
            "fixture",
            RuntimeMode::Client,
            NetworkRequest::Send {
                registration_id: id.into(),
                client_id: None,
                packet: "login.hello".into(),
                data: json!({"name":"Alex"}),
            },
        )
        .expect_err("no transport");
    assert!(error.to_string().contains("transport is not implemented"));
    registry.remove_mod("fixture");
    assert!(
        registry
            .request(
                "fixture",
                RuntimeMode::Client,
                request(id, "login.hello", json!({"name":"Alex"}))
            )
            .is_err()
    );
}
#[test]
fn client_data_defaults_are_independent_and_mutations_are_owner_scoped() {
    let mut registry = NetworkRegistry::default();
    let info = registry
        .request(
            "fixture",
            RuntimeMode::Server,
            NetworkRequest::Register {
                specification: specification(),
                side: RuntimeMode::Server,
                client_data: Some(client_data()),
            },
        )
        .expect("register");
    let id = info["registration_id"].as_str().expect("handle");
    assert_eq!(
        registry.attach_client(id, "a").expect("client a"),
        json!({"tags":[]})
    );
    assert_eq!(
        registry.attach_client(id, "b").expect("client b"),
        json!({"tags":[]})
    );
    let update = |data| NetworkRequest::UpdateClientData {
        registration_id: id.into(),
        client_id: "a".into(),
        data,
    };
    assert!(
        registry
            .request("other", RuntimeMode::Server, update(json!({"tags":["x"]})))
            .is_err()
    );
    assert!(
        registry
            .request(
                "fixture",
                RuntimeMode::Server,
                update(json!({"tags":[123]}))
            )
            .is_err()
    );
    registry
        .request(
            "fixture",
            RuntimeMode::Server,
            update(json!({"tags":["player"]})),
        )
        .expect("mutation");
    assert_eq!(
        registry.detach_client(id, "a"),
        Some(json!({"tags":["player"]}))
    );
    assert_eq!(registry.detach_client(id, "b"), Some(json!({"tags":[]})));
    assert!(
        registry
            .request("fixture", RuntimeMode::Server, update(json!({"tags":[]})))
            .is_err()
    );
    assert_eq!(
        registry
            .attach_client(id, "reconnect")
            .expect("fresh client"),
        json!({"tags":[]})
    );
    let invalid = Schema::I32 {
        min: Some(1.5),
        max: None,
    };
    assert!(invalid.validate_definition(0, &mut 100).is_err());
}
