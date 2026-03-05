#[cfg(test)]
mod test {
    use assert_hex::assert_eq_hex;
    use dbc_data::DbcData;

    #[allow(dead_code)]
    #[derive(DbcData, Default)]
    #[dbc_file = "tests/test.dbc"]
    struct Test {
        aligned_le: AlignedLE,
        aligned_be: AlignedBE,
        unaligned_ule: UnalignedUnsignedLE,
        unaligned_ube: UnalignedUnsignedBE,
        unaligned_sle: UnalignedSignedLE,
        unaligned_sbe: UnalignedSignedBE,
        #[dbc_signals = "Bool_A, Bool_H, Float_A"]
        misc: MiscMessage,
        sixty_four_le: SixtyFourBitLE,
        sixty_four_be: SixtyFourBitBE,
        sixty_four_signed: SixtyFourBitSigned,
        grouped: [GroupData1; 3],
        sub_byte: SubByteLE,
        vcu0tx0: VCU0Tx0,
        vcu0tx5: VCU0Tx5,
    }

    #[test]
    fn basic() {
        let mut t = Test::default();

        // invalid length
        assert!(!t.aligned_le.decode(&[0x00]));

        // message ID, DLC constants
        assert_eq!(AlignedLE::ID, 1023);
        assert_eq!(AlignedLE::DLC, 8);
        assert_eq!(MiscMessage::ID, 8191);
        assert_eq!(MiscMessage::DLC, 2);
    }

    #[test]
    fn aligned_unsigned_le() {
        let mut t = Test::default();

        assert!(
            t.aligned_le
                .decode(&[0xfe, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A])
        );
        assert_eq_hex!(t.aligned_le.Signed8, -2);
        assert_eq_hex!(t.aligned_le.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_le.Unsigned16, 0x2001);
        assert_eq_hex!(t.aligned_le.Unsigned32, 0x9A785634);

        let mut pdu: [u8; 8] = [0u8; 8];
        t.aligned_le.Signed8 = -99;
        t.aligned_le.Unsigned8 = 0x33;
        t.aligned_le.Unsigned16 = 0x78bc;
        assert!(t.aligned_le.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x9d);
        assert_eq_hex!(pdu[1], 0x33);
        assert_eq_hex!(pdu[2], 0xbc);
        assert_eq_hex!(pdu[3], 0x78);
    }

    #[test]
    fn aligned_unsigned_be() {
        let mut t = Test::default();

        assert!(
            t.aligned_be
                .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A])
        );
        assert_eq_hex!(t.aligned_be.Signed8, -86);
        assert_eq_hex!(t.aligned_be.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_be.Unsigned16, 0x0120);
        assert_eq_hex!(t.aligned_be.Unsigned32, 0x3456789A);

        let mut pdu: [u8; 8] = [0u8; 8];
        t.aligned_be.Signed8 = 12;
        t.aligned_be.Unsigned8 = 0x77;
        t.aligned_be.Unsigned16 = 0x78bc;
        t.aligned_be.Unsigned32 = 0x1234FEDC;
        assert!(t.aligned_be.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x0C);
        assert_eq_hex!(pdu[1], 0x77);
        assert_eq_hex!(pdu[2], 0x78);
        assert_eq_hex!(pdu[3], 0xbc);
        assert_eq_hex!(pdu[4], 0x12);
        assert_eq_hex!(pdu[5], 0x34);
        assert_eq_hex!(pdu[6], 0xFE);
        assert_eq_hex!(pdu[7], 0xDC);
    }

    #[test]
    fn unaligned_unsigned_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(
            t.unaligned_ule
                .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd])
        );
        assert_eq_hex!(t.unaligned_ule.Unsigned15, 0x2E74);
        assert_eq_hex!(t.unaligned_ule.Unsigned23, 0x7C0C48);
        assert_eq_hex!(t.unaligned_ule.Unsigned3, 6u8);

        let mut pdu: [u8; 8] = [0xffu8; 8];
        t.unaligned_ule.Unsigned15 = 0x5af5;
        t.unaligned_ule.Unsigned23 = 0x3C0C49;
        t.unaligned_ule.Unsigned3 = 0x2;
        assert!(t.unaligned_ule.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu, [0xffu8, 0xd7, 0x27, 0x31, 0xf0, 0xae, 0xd7, 0xfe]);
    }

    #[test]
    fn unaligned_unsigned_be() {
        let mut t = Test::default();

        // various unaligned values
        let data = [0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77];
        assert!(t.unaligned_ube.decode(&data));
        assert_eq_hex!(t.unaligned_ube.Unsigned3, 2u8);
        assert_eq_hex!(t.unaligned_ube.Unsigned15, 0x4383);
        assert_eq_hex!(t.unaligned_ube.Unsigned23, 0x1F031F);

        // round-trip encode
        let mut pdu = [0u8; 8];
        assert!(t.unaligned_ube.encode(pdu.as_mut_slice()));
        let mut t2 = Test::default();
        assert!(t2.unaligned_ube.decode(&pdu));
        assert_eq_hex!(t2.unaligned_ube.Unsigned3, 2u8);
        assert_eq_hex!(t2.unaligned_ube.Unsigned15, 0x4383);
        assert_eq_hex!(t2.unaligned_ube.Unsigned23, 0x1F031F);
    }

    #[test]
    fn unaligned_signed_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(
            t.unaligned_sle
                .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd])
        );
        assert_eq_hex!(t.unaligned_sle.Signed15, 0x2E74);
        assert_eq_hex!(t.unaligned_sle.Signed23, 0xFFFC0C48u32 as i32);
        assert_eq!(t.unaligned_sle.Signed3, -2);
    }

    #[test]
    fn unaligned_signed_be() {
        let mut t = Test::default();

        // various unaligned values
        assert!(
            t.unaligned_sbe
                .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77])
        );
        assert_eq_hex!(t.unaligned_sbe.Signed3, 2);
        assert_eq_hex!(t.unaligned_sbe.Signed15, 0xC383u16 as i16);
        assert_eq_hex!(t.unaligned_sbe.Signed23, 0x1F031F);
    }

    #[test]
    fn misc() {
        let mut t = Test::default();

        // booleans
        assert!(t.misc.decode(&[0x82, 0x20]));
        assert!(!t.misc.Bool_A);
        assert!(t.misc.Bool_H);
        assert_eq!(t.misc.Float_A, 16.25);

        let mut pdu: [u8; 2] = [0u8; 2];
        t.misc.Bool_A = true;
        t.misc.Float_A = 20.75;
        assert!(t.misc.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x81);
        assert_eq_hex!(pdu[1], 0x29);
    }

    #[test]
    fn sixty_four_bit() {
        let mut t = Test::default();

        // 64-bit unsigned little-endian
        assert!(
            t.sixty_four_le
                .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
        );

        assert_eq!(t.sixty_four_le.SixtyFour, 0x8877665544332211);

        // 64-bit unsigned big-endian
        assert!(
            t.sixty_four_be
                .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
        );

        assert_eq_hex!(t.sixty_four_be.SixtyFour, 0x1122334455667788);

        // 64-bit signed little-endian
        assert!(
            t.sixty_four_signed
                .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
        );

        assert_eq!(t.sixty_four_signed.SixtyFour, -8613303245920329199);
    }

    #[test]
    fn extract() {
        let data: [u8; 1] = [0x87u8];
        let value = i8::from_le_bytes(data);
        assert_eq!(value, -121);
    }

    #[test]
    fn sub_byte_le() {
        let mut t = Test::default();

        // Test sub-byte nibble signals (the pattern from amiga_flex)
        // Byte 0: Nibble0 = 0xA (bits 0-3), Nibble1 = 0x5 (bits 4-7) → 0x5A
        // Byte 1: Nibble2 = 0x3 (bits 0-3), Nibble3 = 0xC (bits 4-7) → 0xC3
        // Bytes 2-4: TwelveBit = 0xABC (bits 16-27), TwelveBit2 = 0xDEF (bits 28-39) → 0xEFABC_D
        // Byte 5: Signed4 = -3 (bits 40-43), Unsigned12at44 high nibble (bits 44-47)
        // Byte 6: Unsigned12at44 low byte (bits 48-55) = ...
        // Byte 7: Unsigned8 = 0x42

        // Build test data manually:
        // Nibble0=0xA, Nibble1=0x5 → byte0 = 0x5A
        // Nibble2=0x3, Nibble3=0xC → byte1 = 0xC3
        // TwelveBit=0xABC: start=16, 12 bits → byte2[7:0] + byte3[3:0]
        //   byte2 = 0xBC, byte3 low nibble = 0xA
        // TwelveBit2=0xDEF: start=28, 12 bits → byte3[7:4] + byte4[7:0]
        //   byte3 high nibble = 0xF, byte4 = 0xDE
        //   byte3 = 0xFA
        // Signed4=-3: start=40, 4 bits → byte5[3:0] = 0xD (two's complement of -3 in 4 bits)
        // Unsigned12at44=0x789: start=44, 12 bits → byte5[7:4] + byte6[7:0]
        //   byte5 high nibble = 0x9, byte6 = 0x78
        //   byte5 = 0x9D
        // Unsigned8=0x42: byte7 = 0x42

        let data: [u8; 8] = [0x5A, 0xC3, 0xBC, 0xFA, 0xDE, 0x9D, 0x78, 0x42];
        assert!(t.sub_byte.decode(&data));
        assert_eq_hex!(t.sub_byte.Nibble0, 0xA);
        assert_eq_hex!(t.sub_byte.Nibble1, 0x5);
        assert_eq_hex!(t.sub_byte.Nibble2, 0x3);
        assert_eq_hex!(t.sub_byte.Nibble3, 0xC);
        assert_eq_hex!(t.sub_byte.TwelveBit, 0xABC);
        assert_eq_hex!(t.sub_byte.TwelveBit2, 0xDEF);
        assert_eq!(t.sub_byte.Signed4, -3);
        assert_eq_hex!(t.sub_byte.Unsigned12at44, 0x789);
        assert_eq_hex!(t.sub_byte.Unsigned8, 0x42);

        // Round-trip encode
        let mut pdu = [0u8; 8];
        t.sub_byte.Nibble0 = 0xA;
        t.sub_byte.Nibble1 = 0x5;
        t.sub_byte.Nibble2 = 0x3;
        t.sub_byte.Nibble3 = 0xC;
        t.sub_byte.TwelveBit = 0xABC;
        t.sub_byte.TwelveBit2 = 0xDEF;
        t.sub_byte.Signed4 = -3;
        t.sub_byte.Unsigned12at44 = 0x789;
        t.sub_byte.Unsigned8 = 0x42;
        assert!(t.sub_byte.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu, data);
    }

    #[test]
    fn amiga_flex_vcu0tx0() {
        let mut t = Test::default();

        // Values from amiga-dbcs round-trip test
        t.vcu0tx0.ctrl_mode_act = 5;
        t.vcu0tx0.ctrl_counter = 10;
        t.vcu0tx0.drive_state_act = 2;
        t.vcu0tx0.drive_mode_act = 3;
        t.vcu0tx0.vcu_ctrl_options1 = 0xAA;
        t.vcu0tx0.vcu_ctrl_options2 = 0x55;
        t.vcu0tx0.steer_value = 1234;
        t.vcu0tx0.velocity_mms = -5678;

        // Encode
        let mut pdu = [0u8; 8];
        assert!(t.vcu0tx0.encode(pdu.as_mut_slice()));

        // Decode into fresh struct and verify round-trip
        let mut t2 = Test::default();
        assert!(t2.vcu0tx0.decode(&pdu));
        assert_eq!(t2.vcu0tx0.ctrl_mode_act, 5);
        assert_eq!(t2.vcu0tx0.ctrl_counter, 10);
        assert_eq!(t2.vcu0tx0.drive_state_act, 2);
        assert_eq!(t2.vcu0tx0.drive_mode_act, 3);
        assert_eq!(t2.vcu0tx0.vcu_ctrl_options1, 0xAA);
        assert_eq!(t2.vcu0tx0.vcu_ctrl_options2, 0x55);
        assert_eq!(t2.vcu0tx0.steer_value, 1234);
        assert_eq!(t2.vcu0tx0.velocity_mms, -5678);

        // Edge case: max values for 4-bit nibbles
        t.vcu0tx0.ctrl_mode_act = 0xF;
        t.vcu0tx0.ctrl_counter = 0xF;
        t.vcu0tx0.drive_state_act = 0xF;
        t.vcu0tx0.drive_mode_act = 0xF;
        t.vcu0tx0.vcu_ctrl_options1 = 0xFF;
        t.vcu0tx0.vcu_ctrl_options2 = 0xFF;
        t.vcu0tx0.steer_value = 32767;
        t.vcu0tx0.velocity_mms = -32768;

        let mut pdu = [0u8; 8];
        assert!(t.vcu0tx0.encode(pdu.as_mut_slice()));
        let mut t2 = Test::default();
        assert!(t2.vcu0tx0.decode(&pdu));
        assert_eq!(t2.vcu0tx0.ctrl_mode_act, 0xF);
        assert_eq!(t2.vcu0tx0.ctrl_counter, 0xF);
        assert_eq!(t2.vcu0tx0.drive_state_act, 0xF);
        assert_eq!(t2.vcu0tx0.drive_mode_act, 0xF);
        assert_eq!(t2.vcu0tx0.steer_value, 32767);
        assert_eq!(t2.vcu0tx0.velocity_mms, -32768);
    }

    #[test]
    fn amiga_flex_vcu0tx5() {
        let mut t = Test::default();

        // Values from amiga-dbcs round-trip test
        t.vcu0tx5.features = 0x42;
        t.vcu0tx5.wheel_dia_mm = 500;
        t.vcu0tx5.gear_ratio = 10;
        t.vcu0tx5.wheel_base_mm = 2000;
        t.vcu0tx5.wheel_track_mm = 1500;

        let mut pdu = [0u8; 8];
        assert!(t.vcu0tx5.encode(pdu.as_mut_slice()));

        let mut t2 = Test::default();
        assert!(t2.vcu0tx5.decode(&pdu));
        assert_eq!(t2.vcu0tx5.features, 0x42);
        assert_eq!(t2.vcu0tx5.wheel_dia_mm, 500);
        assert_eq!(t2.vcu0tx5.gear_ratio, 10);
        assert_eq!(t2.vcu0tx5.wheel_base_mm, 2000);
        assert_eq!(t2.vcu0tx5.wheel_track_mm, 1500);

        // Edge case: max values for 12-bit fields
        t.vcu0tx5.features = 0xFF;
        t.vcu0tx5.wheel_dia_mm = 0xFFF;
        t.vcu0tx5.gear_ratio = 0xFFF;
        t.vcu0tx5.wheel_base_mm = 0xFFFF;
        t.vcu0tx5.wheel_track_mm = 0xFFFF;

        let mut pdu = [0u8; 8];
        assert!(t.vcu0tx5.encode(pdu.as_mut_slice()));
        let mut t2 = Test::default();
        assert!(t2.vcu0tx5.decode(&pdu));
        assert_eq!(t2.vcu0tx5.features, 0xFF);
        assert_eq_hex!(t2.vcu0tx5.wheel_dia_mm, 0xFFF);
        assert_eq_hex!(t2.vcu0tx5.gear_ratio, 0xFFF);
        assert_eq_hex!(t2.vcu0tx5.wheel_base_mm, 0xFFFF);
        assert_eq_hex!(t2.vcu0tx5.wheel_track_mm, 0xFFFF);
    }

    #[test]
    fn grouped() {
        let mut t = Test::default();
        assert!(
            t.grouped[0]
                .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A])
        );
        assert!(t.grouped[0].ValueA == 0x200155AA);
    }
}
