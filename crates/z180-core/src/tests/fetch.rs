use super::*;
#[derive(Default)]
struct FetchBus {
    events: Vec<(char, u32)>,
    program: Vec<u8>,
    fail: bool,
}
impl HostBus for FetchBus {
    type Error = i32;
    fn opcode_fetch(&mut self, a: u32) {
        self.events.push(('F', a));
    }
    fn mem_read(&mut self, a: u32) -> Result<u8, i32> {
        self.events.push(('R', a));
        if self.fail {
            return Err(-42);
        }
        Ok(self.program.get(a as usize).copied().unwrap_or(0))
    }
    fn mem_write(&mut self, a: u32, _: u8) -> Result<(), i32> {
        self.events.push(('W', a));
        Ok(())
    }
    fn io_read(&mut self, _: u16) -> Result<u8, i32> {
        Ok(0)
    }
    fn io_write(&mut self, _: u16, _: u8) -> Result<(), i32> {
        Ok(())
    }
}
fn machine(program: &[u8]) -> Z180<FetchBus> {
    let mut cpu = Z180::new(
        MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x10000,
                kind: RegionKind::External,
            }],
            ..MachineConfig::default()
        },
        FetchBus {
            program: program.to_vec(),
            ..FetchBus::default()
        },
    )
    .unwrap();
    cpu.write_internal_io(DCNTL, 0);
    cpu
}
fn fetches(cpu: &Z180<FetchBus>) -> Vec<u32> {
    cpu.bus
        .events
        .iter()
        .filter_map(|(k, a)| (*k == 'F').then_some(*a))
        .collect()
}
#[test]
fn prefixes_displacements_data_and_read_order() {
    // UM0050 Tables 48-50 and Figures 32-33: opcode versus data cycles.
    for (program, expected, reads) in [
        (vec![0x3e, 0x12], vec![0], vec![0, 1]),
        (vec![0xcb, 0x00], vec![0, 1], vec![0, 1]),
        (vec![0xed, 0x44], vec![0, 1], vec![0, 1]),
        (vec![0xdd, 0x21, 0x34, 0x12], vec![0, 1], vec![0, 1, 2, 3]),
        (vec![0xfd, 0x21, 0x34, 0x12], vec![0, 1], vec![0, 1, 2, 3]),
        (
            vec![0xdd, 0xcb, 1, 0x46],
            vec![0, 1, 3],
            vec![0, 1, 2, 3, 0x101],
        ),
        (
            vec![0xfd, 0xcb, 0xff, 0x46],
            vec![0, 1, 3],
            vec![0, 1, 2, 3, 0xff],
        ),
        (vec![0xdd, 0x7e, 1], vec![0, 1], vec![0, 1, 2, 0x101]),
    ] {
        let mut cpu = machine(&program);
        cpu.set_reg(Reg::IX, 0x100);
        cpu.set_reg(Reg::IY, 0x100);
        cpu.try_step().unwrap();
        assert_eq!(fetches(&cpu), expected, "{program:02x?}");
        let actual: Vec<_> = cpu
            .bus
            .events
            .iter()
            .filter_map(|(k, a)| (*k == 'R').then_some(*a))
            .collect();
        assert_eq!(actual, reads, "{program:02x?}");
        for (index, event) in cpu.bus.events.iter().enumerate() {
            if event.0 == 'F' {
                assert_eq!(cpu.bus.events[index + 1], ('R', event.1));
            }
        }
    }
}
#[test]
fn halt_sleep_interrupt_and_dma_accesses() {
    // UM0050 pp.32-33: dummy reads/inactive bus; pp.73-75: interrupt entry.
    let mut halt = machine(&[0x76]);
    halt.try_step().unwrap();
    halt.bus.events.clear();
    halt.try_step().unwrap();
    assert_eq!(halt.bus.events, vec![('R', 1)]);
    let mut sleep = machine(&[0xed, 0x76]);
    sleep.try_step().unwrap();
    sleep.bus.events.clear();
    sleep.try_step().unwrap();
    assert!(sleep.bus.events.is_empty());
    let mut nmi = machine(&[0]);
    nmi.set_reg(Reg::SP, 0x4000);
    nmi.set_nmi(true);
    nmi.try_step().unwrap();
    assert_eq!(nmi.bus.events, vec![('R', 0), ('W', 0x3fff), ('W', 0x3ffe)]);
    nmi.bus.events.clear();
    nmi.try_step().unwrap();
    assert_eq!(fetches(&nmi), vec![0x66]);
    let mut irq = machine(&[0]);
    irq.set_reg(Reg::SP, 0x4000);
    irq.set_iff1(true);
    irq.write_internal_io(ITC, 4);
    irq.set_irq(IrqLine::Int2, true);
    irq.try_step().unwrap();
    assert_eq!(
        irq.bus.events,
        vec![('W', 0x3fff), ('W', 0x3ffe), ('R', 2), ('R', 3)]
    );
    let mut dma = machine(&[0]);
    dma.write_internal_io(SAR0H, 0x20);
    dma.write_internal_io(DAR0H, 0x30);
    dma.write_internal_io(BCR0L, 1);
    dma.write_internal_io(DMODE, 0);
    dma.write_internal_io(DSTAT, 0x60);
    dma.try_step().unwrap();
    assert_eq!(
        dma.bus.events,
        vec![('R', 0x2000), ('W', 0x3000), ('F', 0), ('R', 0)]
    );
}
#[test]
fn mapping_owned_memory_and_errors() {
    let mut cpu = machine(&[]);
    cpu.write_internal_io(CBAR, 0);
    cpu.write_internal_io(CBR, 1);
    cpu.set_ext_mapper(Some(|a| a + 0x1000));
    cpu.try_step().unwrap();
    assert_eq!(cpu.bus.events, vec![('F', 0x2000), ('R', 0x2000)]);
    let mut owned = Z180::new(
        MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 4096,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        },
        FetchBus::default(),
    )
    .unwrap();
    owned.try_step().unwrap();
    assert_eq!(owned.bus.events, vec![('F', 0)]);
    let mut failed = machine(&[0xdd, 0x21]);
    failed.bus.fail = true;
    assert_eq!(failed.try_step(), Err(-42));
    assert_eq!(failed.bus.events, vec![('F', 0), ('R', 0)]);
}
