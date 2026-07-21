use crate::Variant;

pub(crate) const IO_REGISTER_COUNT: usize = 0x40;
pub(crate) const DCNTL: usize = 0x32;
pub(crate) const IL: usize = 0x33;
pub(crate) const ITC: usize = 0x34;
pub(crate) const CBR: usize = 0x38;
pub(crate) const BBR: usize = 0x39;
pub(crate) const CBAR: usize = 0x3a;
pub(crate) const ICR: usize = 0x3f;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Availability {
    Both,
    Z8S180,
    Reserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadEffect {
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WriteEffect {
    None,
    Dstat,
    Itc,
    Mmu,
    Rdr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IoRegSpec {
    pub(crate) reset: u8,
    pub(crate) read_mask: u8,
    pub(crate) write_mask: u8,
    pub(crate) availability: Availability,
    pub(crate) read_effect: ReadEffect,
    pub(crate) write_effect: WriteEffect,
}

impl IoRegSpec {
    pub(crate) fn is_available(self, variant: Variant) -> bool {
        match self.availability {
            Availability::Both => true,
            Availability::Z8S180 => variant == Variant::Z8S180,
            Availability::Reserved => false,
        }
    }
}

const NONE: ReadEffect = ReadEffect::None;
const BOTH: Availability = Availability::Both;
const S180: Availability = Availability::Z8S180;
const RSV: Availability = Availability::Reserved;
const STORE: WriteEffect = WriteEffect::None;

// UM0050 Tables 6-7 and the register descriptions referenced by those tables.
// Indeterminate reset bits are initialized to zero for deterministic emulation.
pub(crate) const IO_REG_SPECS: [IoRegSpec; IO_REGISTER_COUNT] = [
    // 00 CNTLA0
    IoRegSpec {
        reset: 0x10,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 01 CNTLA1
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 02 CNTLB0
    IoRegSpec {
        reset: 0x07,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 03 CNTLB1
    IoRegSpec {
        reset: 0x07,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 04 STAT0
    IoRegSpec {
        reset: 0x02,
        read_mask: 0xff,
        write_mask: 0x09,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 05 STAT1
    IoRegSpec {
        reset: 0x02,
        read_mask: 0xff,
        write_mask: 0x0d,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 06 TDR0
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 07 TDR1
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 08 RDR0
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Rdr,
    },
    // 09 RDR1
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Rdr,
    },
    // 0A CNTR
    IoRegSpec {
        reset: 0x07,
        read_mask: 0xf7,
        write_mask: 0x77,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 0B TRD
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 0C TMDR0L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 0D TMDR0H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 0E RLDR0L
    IoRegSpec {
        reset: 0xff,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 0F RLDR0H
    IoRegSpec {
        reset: 0xff,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 10 TCR
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0x3f,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 11 reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 12 ASEXT0 (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xfd,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 13 ASEXT1 (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x9f,
        write_mask: 0x9d,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 14 TMDR1L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 15 TMDR1H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 16 RLDR1L
    IoRegSpec {
        reset: 0xff,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 17 RLDR1H
    IoRegSpec {
        reset: 0xff,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 18 FRC
    IoRegSpec {
        reset: 0xff,
        read_mask: 0xff,
        write_mask: 0x00,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 19 reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1A ASTC0L (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1B ASTC0H (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1C ASTC1L (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1D ASTC1H (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1E CMR (Z8S180)
    IoRegSpec {
        reset: 0x7f,
        read_mask: 0xff,
        write_mask: 0x80,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 1F CCR (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 20 SAR0L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 21 SAR0H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 22 SAR0B
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x0f,
        write_mask: 0x0f,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 23 DAR0L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 24 DAR0H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 25 DAR0B
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x0f,
        write_mask: 0x0f,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 26 BCR0L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 27 BCR0H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 28 MAR1L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 29 MAR1H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2A MAR1B
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x0f,
        write_mask: 0x0f,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2B IAR1L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2C IAR1H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2D IAR1B (Z8S180)
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xcf,
        write_mask: 0xcf,
        availability: S180,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2E BCR1L
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 2F BCR1H
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 30 DSTAT
    IoRegSpec {
        reset: 0x30,
        read_mask: 0xfd,
        write_mask: 0xfc,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Dstat,
    },
    // 31 DMODE
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x3e,
        write_mask: 0x3e,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 32 DCNTL
    IoRegSpec {
        reset: 0xf0,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 33 IL
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xe0,
        write_mask: 0xe0,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 34 ITC
    IoRegSpec {
        reset: 0x01,
        read_mask: 0xc7,
        write_mask: 0x87,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Itc,
    },
    // 35 reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 36 RCR
    IoRegSpec {
        reset: 0xc0,
        read_mask: 0xc3,
        write_mask: 0xc3,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 37 reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 38 CBR
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Mmu,
    },
    // 39 BBR
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Mmu,
    },
    // 3A CBAR
    IoRegSpec {
        reset: 0xf0,
        read_mask: 0xff,
        write_mask: 0xff,
        availability: BOTH,
        read_effect: NONE,
        write_effect: WriteEffect::Mmu,
    },
    // 3B reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 3C reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 3D reserved
    IoRegSpec {
        reset: 0x00,
        read_mask: 0x00,
        write_mask: 0x00,
        availability: RSV,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 3E OMCR
    IoRegSpec {
        reset: 0xe0,
        read_mask: 0xa0,
        write_mask: 0xe0,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
    // 3F ICR
    IoRegSpec {
        reset: 0x00,
        read_mask: 0xe0,
        write_mask: 0xe0,
        availability: BOTH,
        read_effect: NONE,
        write_effect: STORE,
    },
];
