use super::*;

#[test]
fn fallible_bus_read_aborts_before_later_write_and_instruction_commit() {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x1000,
            kind: RegionKind::External,
        }],
        ..MachineConfig::default()
    };
    let mut cpu = Z180::new(
        config,
        FailingBus {
            program: [0x3e, 0x5a, 0x32, 0x00, 0x08],
            writes: Vec::new(),
        },
    )
    .expect("valid external-memory machine");
    assert!(cpu.try_step().is_ok());
    let cycle_count = cpu.cycle_count();

    assert_eq!(cpu.try_step(), Err("operand read failed"));
    assert!(cpu.bus.writes.is_empty());
    assert_eq!(cpu.reg(Reg::PC), 0x0002);
    assert_eq!(cpu.cycle_count(), cycle_count);
}

#[test]
fn mmu_reset_and_internal_io_writes_recompute_all_pages() {
    let mut cpu = mmu_machine();
    for logical_page in 0_u16..16 {
        let logical = (logical_page << 12) | 0x0a5;
        assert_eq!(cpu.mmu_translate(logical), u32::from(logical));
    }

    for (pc, register, value) in [(0_u16, CBR, 0x80_u8), (3, CBAR, 0xa4_u8), (6, BBR, 0x40_u8)] {
        let physical = cpu.mmu_translate(pc);
        cpu.mem_poke(physical, 0xed);
        cpu.mem_poke(physical + 1, 0x01);
        cpu.mem_poke(physical + 2, register as u8);
        cpu.set_reg(Reg::BC, u16::from(value) << 8);
        assert_ne!(cpu.step(), 0);
    }

    assert_eq!(cpu.io_reg_peek(CBR as u8), 0x80);
    assert_eq!(cpu.io_reg_peek(BBR as u8), 0x40);
    assert_eq!(cpu.io_reg_peek(CBAR as u8), 0xa4);
    assert_eq!(cpu.mmu_translate(0x30a5), 0x030a5);
    assert_eq!(cpu.mmu_translate(0x40a5), 0x440a5);
    assert_eq!(cpu.mmu_translate(0x90a5), 0x490a5);
    assert_eq!(cpu.mmu_translate(0xa0a5), 0x8a0a5);

    cpu.reset();
    assert_eq!(cpu.io_reg_peek(CBR as u8), 0x00);
    assert_eq!(cpu.io_reg_peek(BBR as u8), 0x00);
    assert_eq!(cpu.io_reg_peek(CBAR as u8), 0xf0);
    for logical_page in 0_u16..16 {
        let logical = (logical_page << 12) | 0x0a5;
        assert_eq!(cpu.mmu_translate(logical), u32::from(logical));
    }
}

#[test]
fn mmu_translates_instruction_fetches_reads_and_writes() {
    let mut cpu = mmu_machine();
    cpu.write_internal_io(CBR, 0x10);
    cpu.write_internal_io(CBAR, 0x00);

    cpu.mem_poke(0x1_0000, 0x7e);
    cpu.mem_poke(0x1_2000, 0x5a);
    cpu.set_reg(Reg::HL, 0x2000);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.reg(Reg::AF) >> 8, 0x5a);

    cpu.mem_poke(0x1_0001, 0x77);
    cpu.set_reg(Reg::PC, 1);
    cpu.set_reg(Reg::HL, 0x3000);
    cpu.set_reg(Reg::AF, 0xa500);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.mem_peek(0x1_3000), 0xa5);
    assert_eq!(cpu.mem_peek(0x3000), 0x00);
}

#[test]
fn remap_replaces_pages_and_preserves_unaffected_ram_storage() {
    let config = MachineConfig {
        regions: vec![
            RegionDef {
                base: 0,
                size: 0x3000,
                kind: RegionKind::Ram,
            },
            RegionDef {
                base: 0x4000,
                size: 0x1000,
                kind: RegionKind::Ram,
            },
        ],
        ..MachineConfig::default()
    };
    let mut cpu = Z180::new(config, NullBus).expect("split RAM configuration must be valid");
    cpu.ram_region_mut(0)
        .expect("the first RAM region must be exposed")[0] = 0x5a;
    cpu.mem_poke(0x2000, 0x66);
    let first_pointer = cpu
        .ram_region(0)
        .expect("the first RAM region must be exposed")
        .as_ptr();
    assert_eq!(cpu.ram_regions(), vec![(0, 0x3000), (0x4000, 0x1000)]);
    assert_eq!(
        cpu.ram_region(0x4000).map(<[u8]>::len),
        Some(0x1000),
        "a separate nonzero-base RAM region must be exposed"
    );
    cpu.remap(0, 0, RegionKind::Ram)
        .expect("a zero-sized remap must be a no-op");
    assert_eq!(
        cpu.ram_region(0)
            .expect("a zero-sized remap must preserve RAM")
            .as_ptr(),
        first_pointer
    );

    cpu.remap(0x4000, 0x1000, RegionKind::External)
        .expect("an unrelated RAM region must remap");
    assert_eq!(
        cpu.ram_region(0)
            .expect("the unaffected RAM region must remain exposed")
            .as_ptr(),
        first_pointer
    );
    cpu.remap(0x1000, 0x1000, RegionKind::Rom(vec![0xa5; 0x1000]))
        .expect("the middle RAM page must remap to ROM");
    assert_eq!(cpu.ram_regions(), vec![(0, 0x1000), (0x2000, 0x1000)]);
    assert_eq!(cpu.ram_region(0).map(<[u8]>::len), Some(0x1000));
    assert_eq!(cpu.ram_region(0x2000).map(<[u8]>::len), Some(0x1000));
    assert_eq!(cpu.mem_peek(0), 0x5a);
    assert_eq!(cpu.mem_peek(0x1000), 0xa5);
    assert_eq!(cpu.mem_peek(0x2000), 0x66);

    let error = cpu
        .remap(0, 0x1000, RegionKind::Rom(vec![0; 1]))
        .expect_err("invalid remaps must be atomic");
    assert_eq!(
        error,
        ConfigError::RomSizeMismatch {
            region_size: 0x1000,
            data_size: 1,
        }
    );
    assert_eq!(cpu.mem_peek(0), 0x5a);

    cpu.remap(0x1000, 0x1000, RegionKind::Ram)
        .expect("the ROM page must remap to fresh RAM");
    assert_eq!(cpu.mem_peek(0x1000), 0, "new RAM is zero initialized");
}

#[test]
fn debugger_memory_access_stays_with_core_owned_storage() {
    let config = MachineConfig {
        unmapped_read: 0xa5,
        regions: vec![
            RegionDef {
                base: 0,
                size: 0x1000,
                kind: RegionKind::Ram,
            },
            RegionDef {
                base: 0x1000,
                size: 0x1000,
                kind: RegionKind::Rom(vec![0x22; 0x1000]),
            },
            RegionDef {
                base: 0x2000,
                size: 0x1000,
                kind: RegionKind::External,
            },
        ],
        ..MachineConfig::default()
    };
    let mut cpu = Z180::new(config, RecordingBus::default()).expect("memory layout must be valid");
    let _ = cpu.add_mem_watch(0, 0x4000, WatchKind::Both);

    cpu.mem_poke(0, 0x11);
    cpu.mem_poke(0x1000, 0x33);
    cpu.mem_poke(0x2000, 0x44);
    cpu.mem_poke(0x3000, 0x55);

    assert_eq!(cpu.mem_peek(0), 0x11);
    assert_eq!(cpu.mem_peek(0x1000), 0x22);
    assert_eq!(cpu.mem_peek(0x2000), 0xa5);
    assert_eq!(cpu.mem_peek(0x3000), 0xa5);
    assert!(cpu.bus.memory_writes.is_empty());
    assert!(cpu.drain_events().is_empty());
}

#[test]
fn external_mapper_function_and_table_apply_after_mmu_translation() {
    fn bank_one(physical: u32) -> u32 {
        physical + 0x1_0000
    }

    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0x1_0000,
            size: 0x1000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    let mut cpu = Z180::new(config, NullBus).expect("banked RAM configuration must be valid");
    cpu.mem_poke(0x1_0000, 0x3e);
    cpu.mem_poke(0x1_0001, 0x5a);
    cpu.set_ext_mapper(Some(bank_one));
    assert_eq!(cpu.mmu_translate(0), 0, "MMU visibility remains unmapped");
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.reg(Reg::AF) >> 8, 0x5a);

    let error = cpu
        .set_ext_map_table(Some(vec![0; 3]))
        .expect_err("a short mapper table must be rejected");
    assert_eq!(
        error,
        ConfigError::InvalidExtMapTableLength {
            expected: EXT_MAP_TABLE_LEN,
            actual: 3,
        }
    );
    cpu.set_reg(Reg::PC, 0);
    assert_ne!(cpu.step(), 0, "rejection preserves the prior mapper");

    let mut table = (0..EXT_MAP_TABLE_LEN as u32).collect::<Vec<_>>();
    for physical in &mut table[..0x1000] {
        *physical += 0x1_0000;
    }
    cpu.set_ext_map_table(Some(table))
        .expect("a complete mapper table must be accepted");
    cpu.set_reg(Reg::PC, 0);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.reg(Reg::AF) >> 8, 0x5a);

    cpu.set_ext_map_table(None)
        .expect("clearing a mapper table must succeed");
    cpu.set_reg(Reg::PC, 0);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.reg(Reg::PC), 0x0038, "unmapped FFh executes RST 38h");
}

#[test]
fn mmu_boundary_cases_cover_empty_regions_and_one_mibibyte_wrap() {
    let mut cpu = mmu_machine();
    cpu.write_internal_io(CBR, 0x20);
    cpu.write_internal_io(BBR, 0x40);

    cpu.write_internal_io(CBAR, 0x88);
    assert_eq!(cpu.mmu_translate(0x7123), 0x07123, "below BA=CA");
    assert_eq!(cpu.mmu_translate(0x8123), 0x28123, "at BA=CA");

    cpu.write_internal_io(CBAR, 0x80);
    assert_eq!(cpu.mmu_translate(0x0123), 0x40123, "BA=0 bank base");
    assert_eq!(cpu.mmu_translate(0x7123), 0x47123, "BA=0 bank end");
    assert_eq!(cpu.mmu_translate(0x8123), 0x28123, "BA=0 common 1");

    cpu.write_internal_io(CBAR, 0xf4);
    assert_eq!(cpu.mmu_translate(0x3123), 0x03123, "below BA");
    assert_eq!(cpu.mmu_translate(0x4123), 0x44123, "at BA");
    assert_eq!(cpu.mmu_translate(0xe123), 0x4e123, "below CA=F");
    assert_eq!(cpu.mmu_translate(0xf123), 0x2f123, "at CA=F");

    cpu.write_internal_io(CBR, 0xff);
    cpu.write_internal_io(CBAR, 0x00);
    assert_eq!(cpu.mmu_translate(0x1123), 0x00123, "CBR wraps at 1 MiB");
    assert_eq!(cpu.mmu_translate(0xf123), 0x0e123, "CBR wrap keeps page");

    cpu.write_internal_io(BBR, 0xff);
    cpu.write_internal_io(CBAR, 0xf0);
    assert_eq!(cpu.mmu_translate(0x1123), 0x00123, "BBR wraps at 1 MiB");
    assert_eq!(cpu.mmu_translate(0xe123), 0x0d123, "BBR wrap keeps page");
}

proptest! {
    #[test]
    fn mmu_translation_array_matches_closed_form(
        cbr in any::<u8>(),
        bbr in any::<u8>(),
        cbar in any::<u8>(),
        logical in any::<u16>(),
    ) {
        let mut cpu = mmu_machine();
        cpu.write_internal_io(CBR, cbr);
        cpu.write_internal_io(BBR, bbr);
        cpu.write_internal_io(CBAR, cbar);

        let page = u32::from(logical >> 12);
        let ba = u32::from(cbar & 0x0f);
        let ca = u32::from(cbar >> 4);
        let relocation = if page < ba {
            0
        } else if page < ca {
            u32::from(bbr)
        } else {
            u32::from(cbr)
        };
        let expected = (((relocation + page) & 0xff) << 12)
            | u32::from(logical & 0x0fff);

        prop_assert_eq!(cpu.mmu_translate(logical), expected);
    }
}
