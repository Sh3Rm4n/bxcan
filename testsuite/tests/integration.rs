#![no_std]
#![no_main]

use bxcan::{Can, Frame};
use nb::block;
use testsuite::{self, pac, CAN1};

struct State {
    can1: Can<CAN1>,
}

impl State {
    fn init() -> Self {
        let mut periph = defmt::unwrap!(pac::Peripherals::take());
        let (can1, _) = testsuite::init(periph.CAN1, periph.CAN2, &mut periph.RCC);
        let mut can1 = Can::new(can1);
        can1.configure(|c| {
            c.set_loopback(true);
            c.set_silent(true);
        });

        Self { can1 }
    }
}

fn roundtrip_frame(frame: &Frame, state: &mut State) -> bool {
    block!(state.can1.transmit(frame)).unwrap();

    // Wait a short while to ensure the frame is fully received.
    cortex_m::asm::delay(100_000);

    match state.can1.receive() {
        Ok(received) => {
            defmt::assert_eq!(received, *frame);
            true
        }
        Err(nb::Error::WouldBlock) => false,
        Err(nb::Error::Other(e)) => defmt::panic!("{:?}", e),
    }
}

#[defmt_test::tests]
mod tests {
    use bxcan::filter::{BankConfig, ListEntry32, Mask16, Mask32};
    use bxcan::{ExtendedId, Frame, StandardId};

    use super::*;

    #[init]
    fn init() -> State {
        let mut state = State::init();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        drop(filt);
        nb::block!(state.can1.enable()).unwrap();

        state
    }

    // FIXME: This is supposed to run on a device with 2 CAN peripherals.
    /*#[test]
    fn split_filters(state: &mut super::State) {
        let mut filt = state.can1.modify_filters();

        filt.set_split(0);
        defmt::assert_eq!(filt.num_banks(), 0);
        defmt::assert_eq!(filt.slave_filters().num_banks(), 14);

        filt.set_split(1);
        defmt::assert_eq!(filt.num_banks(), 1);
        defmt::assert_eq!(filt.slave_filters().num_banks(), 13);

        filt.set_split(13);
        defmt::assert_eq!(filt.num_banks(), 13);
        defmt::assert_eq!(filt.slave_filters().num_banks(), 1);

        filt.set_split(14);
        defmt::assert_eq!(filt.num_banks(), 14);
        defmt::assert_eq!(filt.slave_filters().num_banks(), 0);
    }*/

    #[test]
    fn basic_roundtrip(state: &mut State) {
        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(0, BankConfig::Mask32(Mask32::accept_all()));
        drop(filt);

        let frame = Frame::new_data(StandardId::new(0).unwrap(), []);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_data(ExtendedId::new(0xFFFF).unwrap(), [1, 2, 3, 4, 5]);
        defmt::assert!(roundtrip_frame(&frame, state));
    }

    #[test]
    fn no_filters_no_frames(state: &mut State) {
        state.can1.modify_filters().clear();

        let frame = Frame::new_data(ExtendedId::new(0).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(StandardId::new(0).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
    }

    #[test]
    fn filter_mask32_std(state: &mut State) {
        let target_id = StandardId::new(42).unwrap();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(0, BankConfig::Mask32(Mask32::frames_with_std_id(target_id)));
        drop(filt);

        // Data frames with matching IDs should be accepted.
        let frame = Frame::new_data(target_id, []);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_data(target_id, [1, 2, 3, 4, 5, 6, 7, 8]);
        defmt::assert!(roundtrip_frame(&frame, state));

        // ...remote frames with the same IDs should also be accepted.
        let frame = Frame::new_remote(target_id, 0);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_remote(target_id, 7);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_remote(target_id, 8);
        defmt::assert!(roundtrip_frame(&frame, state));

        // Different IDs should *not* be received.
        let frame = Frame::new_data(StandardId::new(1000).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Extended IDs that match the filter should be *rejected*.
        let frame = Frame::new_data(ExtendedId::new(target_id.as_raw().into()).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // ...even when shifted upwards to match the standard ID bits.
        let frame = Frame::new_data(
            ExtendedId::new(u32::from(target_id.as_raw()) << 18).unwrap(),
            [],
        );
        defmt::assert!(!roundtrip_frame(&frame, state));
    }

    #[test]
    fn filter_mask32_ext(state: &mut State) {
        let target_id = ExtendedId::new(0).unwrap();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(0, BankConfig::Mask32(Mask32::frames_with_ext_id(target_id)));
        drop(filt);

        // Data frames with matching IDs should be accepted.
        let frame = Frame::new_data(target_id, []);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_data(target_id, [1, 2, 3, 4, 5, 6, 7, 8]);
        defmt::assert!(roundtrip_frame(&frame, state));

        // ...remote frames with the same IDs should also be accepted.
        let frame = Frame::new_remote(target_id, 0);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_remote(target_id, 7);
        defmt::assert!(roundtrip_frame(&frame, state));

        let frame = Frame::new_remote(target_id, 8);
        defmt::assert!(roundtrip_frame(&frame, state));

        // Different IDs should *not* be received.
        let frame = Frame::new_data(ExtendedId::new(1000).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Standard IDs should be *rejected* even if their value matches the filter mask.
        let frame = Frame::new_data(StandardId::new(0).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Different (standard) IDs should *not* be received.
        let frame = Frame::new_data(StandardId::MAX, []);
        defmt::assert!(!roundtrip_frame(&frame, state));
    }

    #[test]
    fn filter_mask16(state: &mut State) {
        let target_id_1 = StandardId::new(16).unwrap();
        let target_id_2 = StandardId::new(17).unwrap();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(
            0,
            BankConfig::Mask16([
                Mask16::frames_with_std_id(target_id_1),
                Mask16::frames_with_std_id(target_id_2),
            ]),
        );
        drop(filt);

        // Data frames with matching IDs should be accepted.
        let frame = Frame::new_data(target_id_1, []);
        defmt::assert!(roundtrip_frame(&frame, state));
        let frame = Frame::new_data(target_id_2, []);
        defmt::assert!(roundtrip_frame(&frame, state));

        // Incorrect IDs should be rejected.
        let frame = Frame::new_data(StandardId::new(15).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(StandardId::new(18).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Extended frames with the same ID are rejected, because the upper bits do not match.
        let frame = Frame::new_data(ExtendedId::new(16).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(ExtendedId::new(17).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Extended frames whose upper bits match the filter value are *still* rejected.
        let frame = Frame::new_data(ExtendedId::new(16 << 18).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(ExtendedId::new(17 << 18).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
    }

    /// `List32` filter mode accepting standard CAN frames.
    #[test]
    fn filter_list32_std(state: &mut State) {
        let target_id_1 = StandardId::MAX;
        let target_id_2 = StandardId::new(42).unwrap();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(
            0,
            BankConfig::List32([
                ListEntry32::data_frames_with_id(target_id_1),
                ListEntry32::remote_frames_with_id(target_id_2),
            ]),
        );
        drop(filt);

        // Frames with matching IDs should be accepted.
        let frame = Frame::new_data(target_id_1, []);
        defmt::assert!(roundtrip_frame(&frame, state));
        let frame = Frame::new_remote(target_id_2, 8);
        defmt::assert!(roundtrip_frame(&frame, state));

        // Date/Remote frame type must match.
        let frame = Frame::new_remote(target_id_1, 8);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(target_id_2, []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Frames with matching, but *extended* IDs should be rejected.
        let frame = Frame::new_data(ExtendedId::new(target_id_1.as_raw().into()).unwrap(), []);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_remote(ExtendedId::new(target_id_2.as_raw().into()).unwrap(), 8);
        defmt::assert!(!roundtrip_frame(&frame, state));
    }

    /// `List32` filter mode accepting extended CAN frames.
    #[test]
    fn filter_list32_ext(state: &mut State) {
        let target_id_1 = ExtendedId::MAX;
        let target_id_2 = ExtendedId::new(42).unwrap();

        let mut filt = state.can1.modify_filters();
        filt.clear();
        filt.enable_bank(
            0,
            BankConfig::List32([
                ListEntry32::data_frames_with_id(target_id_1),
                ListEntry32::remote_frames_with_id(target_id_2),
            ]),
        );
        drop(filt);

        // Frames with matching IDs should be accepted.
        let frame = Frame::new_data(target_id_1, []);
        defmt::assert!(roundtrip_frame(&frame, state));
        let frame = Frame::new_remote(target_id_2, 8);
        defmt::assert!(roundtrip_frame(&frame, state));

        // Date/Remote frame type must match.
        let frame = Frame::new_remote(target_id_1, 8);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_data(target_id_2, []);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Other IDs are rejected.
        let frame = Frame::new_remote(ExtendedId::new(43).unwrap(), 1);
        defmt::assert!(!roundtrip_frame(&frame, state));
        let frame = Frame::new_remote(ExtendedId::new(41).unwrap(), 1);
        defmt::assert!(!roundtrip_frame(&frame, state));

        // Matching standard IDs are rejected.
        let frame = Frame::new_remote(StandardId::new(42).unwrap(), 1);
        defmt::assert!(!roundtrip_frame(&frame, state));
    }
}