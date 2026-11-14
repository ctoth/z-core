use super::*;

#[derive(Default)]
struct ContextBus {
    accesses: Vec<(u32, BusContext)>,
    program: Vec<u8>,
}
impl HostBus for ContextBus {
    type Error = Infallible;
    fn mem_read(&mut self, _: u32) -> Result<u8, Infallible> {
        unreachable!()
    }
    fn mem_write(&mut self, _: u32, _: u8) -> Result<(), Infallible> {
        unreachable!()
    }
    fn io_read(&mut self, _: u16) -> Result<u8, Infallible> {
        unreachable!()
    }
    fn io_write(&mut self, _: u16, _: u8) -> Result<(), Infallible> {
        unreachable!()
    }
    fn mem_read_context(&mut self, a: u32, c: BusContext) -> Result<u8, Infallible> {
        self.accesses.push((a, c));
        Ok(self.program.get(a as usize).copied().unwrap_or(0))
    }
    fn mem_write_context(&mut self, a: u32, _: u8, c: BusContext) -> Result<(), Infallible> {
        self.accesses.push((a, c));
        Ok(())
    }
    fn io_read_context(&mut self, a: u16, c: BusContext) -> Result<u8, Infallible> {
        self.accesses.push((u32::from(a), c));
        Ok(0)
    }
    fn io_write_context(&mut self, a: u16, _: u8, c: BusContext) -> Result<(), Infallible> {
        self.accesses.push((u32::from(a), c));
        Ok(())
    }
}
fn machine(program: &[u8]) -> Z180<ContextBus> {
    let mut cpu = Z180::new(
        MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x10000,
                kind: RegionKind::External,
            }],
            ..MachineConfig::default()
        },
        ContextBus {
            program: program.to_vec(),
            ..ContextBus::default()
        },
    )
    .unwrap();
    cpu.write_internal_io(DCNTL, 0);
    cpu
}
#[test]
fn instruction_io_and_reset_context() {
    let mut cpu = machine(&[0x3e, 0x12, 0xd3, 0x80, 0xdb, 0x80, 0x32, 0, 0x20]);
    for pc in [0, 2, 4, 6] {
        let cycles = cpu.cycle_count();
        cpu.bus.accesses.clear();
        cpu.step();
        assert!(!cpu.bus.accesses.is_empty());
        assert!(
            cpu.bus
                .accesses
                .iter()
                .all(|(_, c)| *c == BusContext { cycles, pc })
        );
    }
    let cycles = cpu.cycle_count();
    cpu.reset();
    cpu.bus.accesses.clear();
    cpu.step();
    assert_eq!(cpu.bus.accesses[0], (0, BusContext { cycles, pc: 0 }));
}
#[test]
fn dma_then_cpu_observes_charged_cycles_and_pending_pc() {
    let mut cpu = machine(&[0, 0]);
    cpu.step();
    let cycles = cpu.cycle_count();
    cpu.write_internal_io(SAR0H, 0x20);
    cpu.write_internal_io(DAR0H, 0x30);
    cpu.write_internal_io(BCR0L, 1);
    cpu.write_internal_io(DMODE, 0);
    cpu.write_internal_io(DSTAT, 0x60);
    cpu.bus.accesses.clear();
    cpu.step();
    assert_eq!(
        cpu.bus.accesses,
        vec![
            (0x2000, BusContext { cycles, pc: 1 }),
            (0x3000, BusContext { cycles, pc: 1 }),
            (
                1,
                BusContext {
                    cycles: cycles + 6,
                    pc: 1
                }
            ),
        ]
    );
}
#[test]
fn dma_io_and_interrupt_entry_use_pending_pc() {
    let mut cpu = machine(&[0]);
    cpu.step();
    let cycles = cpu.cycle_count();
    cpu.write_internal_io(SAR0H, 0x20);
    cpu.write_internal_io(DAR0L, 0x80);
    cpu.write_internal_io(BCR0L, 1);
    cpu.write_internal_io(DMODE, 0x30);
    cpu.write_internal_io(DCNTL, 8);
    cpu.write_internal_io(DSTAT, 0x60);
    cpu.set_dreq(0, true);
    cpu.bus.accesses.clear();
    cpu.step();
    assert_eq!(cpu.bus.accesses[1], (0x80, BusContext { cycles, pc: 1 }));
    let cycles = cpu.cycle_count();
    cpu.set_reg(Reg::SP, 0x4000);
    cpu.set_nmi(true);
    cpu.bus.accesses.clear();
    cpu.step();
    assert_eq!(
        cpu.bus.accesses.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
        vec![2, 0x3fff, 0x3ffe]
    );
    assert!(
        cpu.bus
            .accesses
            .iter()
            .all(|(_, c)| *c == BusContext { cycles, pc: 2 })
    );
    assert_eq!(cpu.instruction_pc(), 1);
}
