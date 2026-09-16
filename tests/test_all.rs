//! `dbc_all!` — generate every message and value table in a DBC.

/// Every message in `all.dbc`, with nothing in Rust repeating the list.
mod pump {
    dbc_data::dbc_all!("tests/all.dbc");
}

/// The same DBC the derive tests use. It has 16 messages; the derive struct
/// in `test.rs` names 14 of them.
mod every {
    dbc_data::dbc_all!("tests/test.dbc");
}

#[test]
fn every_message_in_the_file_becomes_a_type() {
    assert_eq!(pump::PumpCommand::ID, 512);
    assert_eq!(pump::PumpState::ID, 513);
    assert_eq!(pump::PumpTemperature::ID, 514);
}

#[test]
fn dlc_and_extended_come_from_the_file() {
    assert_eq!(pump::PumpCommand::DLC, 8);
    assert_eq!(pump::PumpState::DLC, 4);
    assert_eq!(pump::PumpTemperature::DLC, 2);
}

#[test]
fn the_cycle_time_attribute_still_lands() {
    assert_eq!(pump::PumpCommand::CYCLE_TIME, 100);
}

#[test]
fn value_tables_become_enums() {
    assert_eq!(pump::PumpMode::try_from(2), Ok(pump::PumpMode::Running));
    assert!(pump::PumpMode::try_from(9).is_err());
    assert_eq!(u8::from(pump::PumpMode::Fault), 3);
}

/// The point of the macro: a message nothing in Rust asked for is still
/// generated. `GroupData2` and `GroupData3` have no field in `test.rs`'s
/// derive struct.
#[test]
fn a_message_no_struct_field_names_is_generated() {
    assert_eq!(every::GroupData1::ID, 128);
    assert_eq!(every::GroupData2::ID, 129);
    assert_eq!(every::GroupData3::ID, 130);
}

/// `BO_` carries the 29-bit id with `0x80000000` set to mark it extended.
/// The generated `ID` is the id without that flag, and `EXTENDED` is how you
/// tell the two widths apart.
#[test]
fn extended_ids_keep_their_flag() {
    assert_eq!(
        (pump::PumpDiagnostics::ID, pump::PumpDiagnostics::EXTENDED),
        (0x1_0000, true)
    );
    assert_eq!(
        (pump::PumpCommand::ID, pump::PumpCommand::EXTENDED),
        (512, false)
    );
}

/// A `BO_` id above 11 bits without the extended flag is not an extended
/// message — can-dbc reads it as `Standard(u16)` and it truncates. Worth a
/// test now that `dbc_all!` generates every message in a file: a DBC written
/// that way produces a type with a silently wrong `ID`.
///
/// `tests/test.dbc` has `BO_ 65536 VCU0Tx0`, which is this mistake.
#[test]
fn an_unflagged_large_id_truncates_rather_than_becoming_extended() {
    assert_eq!((every::VCU0Tx0::ID, every::VCU0Tx0::EXTENDED), (0, false));
}

/// Encode and decode are the same code the derive emits, so one round trip
/// per direction is enough to show the generated impls are wired up. The
/// layout itself is pinned by the derive tests in `test.rs`.
#[test]
fn generated_types_encode_and_decode() {
    let mut command = pump::PumpCommand {
        enable: true,
        target_rpm: -2,
    };
    let mut data = [0u8; 8];
    assert!(command.encode(&mut data));
    // enable=1 in bit 0 -> 0x01 | -2 little-endian -> FE FF
    assert_eq!(data, [0x01, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let mut decoded = pump::PumpCommand::default();
    assert!(decoded.decode(&data));
    assert!(decoded.enable);
    assert_eq!(decoded.target_rpm, -2);
}

#[test]
fn decode_still_rejects_the_wrong_length() {
    let mut state = pump::PumpState::default();
    assert!(!state.decode(&[0u8; 3]));
    assert!(!state.decode(&[0u8; 5]));
}
