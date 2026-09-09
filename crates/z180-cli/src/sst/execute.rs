use super::*;

#[derive(Default)]
pub(super) struct PortScript {
    pub(super) expected: Vec<PortEvent>,
    pub(super) observed: Vec<PortEvent>,
}

#[derive(Clone, Default)]
pub(super) struct ScriptedBus {
    pub(super) script: Rc<RefCell<PortScript>>,
}

impl HostBus for ScriptedBus {
    type Error = core::convert::Infallible;

    fn mem_read(&mut self, _phys: u32) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
        let mut script = self.script.borrow_mut();
        let value = script
            .expected
            .get(script.observed.len())
            .filter(|event| event.0 == port && event.2 == PortDirection::R)
            .map_or(0xff, |event| event.1);
        script
            .observed
            .push(PortEvent(port, value, PortDirection::R));
        Ok(value)
    }

    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
        self.script
            .borrow_mut()
            .observed
            .push(PortEvent(port, value, PortDirection::W));
        Ok(())
    }
}

pub(super) fn implemented(opcodes: &[u8]) -> bool {
    Z180::<ScriptedBus>::is_instruction_implemented(opcodes)
}

pub(super) fn run_file(
    file: String,
    cases: Vec<TestCase>,
    ignore_r: bool,
    sabotage_ld: bool,
) -> Result<FileReport> {
    let mut file_report = FileReport {
        file,
        pass: 0,
        fail: 0,
        unimplemented: 0,
        failures: Vec::new(),
    };

    for case in cases {
        let expected_ports = case.ports.clone().unwrap_or_default();
        let internal_io_base = if case.kind.is_none() {
            [0x00_u8, 0x40, 0x80, 0xc0]
                .into_iter()
                .find(|base| {
                    expected_ports.iter().all(|event| {
                        let [high, low] = event.0.to_be_bytes();
                        high != 0 || low & 0xc0 != *base
                    })
                })
                .with_context(|| {
                    format!(
                        "{} uses external ports in every possible internal-I/O window",
                        case.name
                    )
                })?
        } else {
            0
        };
        let (mut cpu, port_script) = machine(expected_ports, 0x1_0000)?;
        if internal_io_base != 0 {
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0x01);
            cpu.mem_poke(2, 0x3f);
            cpu.set_reg(Reg::BC, u16::from(internal_io_base) << 8);
            if cpu.step() == 0 || cpu.io_reg_peek(0x3f) != internal_io_base {
                bail!(
                    "failed to relocate the internal-I/O window for {}",
                    case.name
                );
            }
            port_script.borrow_mut().observed.clear();
        }
        load_state(&mut cpu, &case.initial)
            .with_context(|| format!("failed to load initial state for {}", case.name))?;
        let _ = cpu.add_mem_watch(0, 0x1_0000, WatchKind::Write);
        let sabotage = sabotage_ld.then(|| inject_reversed_ld(&mut cpu)).flatten();
        let cycles = cpu.step();
        if let Some(sabotage) = sabotage {
            sabotage.restore_fetch_byte(&mut cpu);
        }
        if cycles == 0 {
            file_report.unimplemented += 1;
            continue;
        }

        let write_failure = if cpu.events_lost() {
            Some(Failure {
                test: case.name.clone(),
                field: "ram.writes".to_owned(),
                expected: "complete write event set".to_owned(),
                actual: "event overflow".to_owned(),
            })
        } else {
            cpu.drain_events().into_iter().find_map(|event| {
                let Event::MemWrite { phys, val, .. } = event else {
                    return None;
                };
                (!case
                    .final_state
                    .ram
                    .iter()
                    .any(|[address, _]| u32::from(*address) == phys))
                .then(|| Failure {
                    test: case.name.clone(),
                    field: format!("ram[{phys:04x}]"),
                    expected: "<unchanged>".to_owned(),
                    actual: format!("{val:02x}"),
                })
            })
        };
        let failure = compare(&cpu, &case, ignore_r)
            .or(write_failure)
            .or_else(|| compare_ports(&port_script.borrow(), &case.name));
        if let Some(failure) = failure {
            file_report.fail += 1;
            file_report.failures.push(failure);
        } else {
            file_report.pass += 1;
        }
    }

    Ok(file_report)
}

#[allow(
    clippy::too_many_lines,
    reason = "the MMU corpus runner keeps programming, all sixteen probes, and final-state comparison in one auditable flow"
)]
pub(super) fn run_mmu_file(
    file: String,
    cases: Vec<TestCase>,
    ignore_r: bool,
) -> Result<FileReport> {
    let mut file_report = FileReport {
        file,
        pass: 0,
        fail: 0,
        unimplemented: 0,
        failures: Vec::new(),
    };

    for case in cases {
        let expected_z180 = case
            .final_state
            .z180
            .as_ref()
            .with_context(|| format!("{} has no final Z180 state", case.name))?;
        let (mut cpu, port_script) = machine(Vec::new(), 0x10_0000)?;
        let mut failure = None;

        for (logical_pc, internal_addr, value) in [
            (0_u16, 0x38_u8, expected_z180.cbr),
            (3_u16, 0x3a_u8, expected_z180.cbar),
            (6_u16, 0x39_u8, expected_z180.bbr),
        ] {
            let physical_pc = cpu.mmu_translate(logical_pc);
            cpu.mem_poke(physical_pc, 0xed);
            cpu.mem_poke(physical_pc + 1, 0x01);
            cpu.mem_poke(physical_pc + 2, internal_addr);
            cpu.set_reg(Reg::PC, logical_pc);
            cpu.set_reg(Reg::BC, u16::from(value) << 8);
            let _ = cpu.step();

            let actual = cpu.io_reg_peek(internal_addr);
            if actual != value {
                failure = Some(Failure {
                    test: case.name.clone(),
                    field: format!("z180.io[{internal_addr:02x}]"),
                    expected: format!("{value:02x}"),
                    actual: format!("{actual:02x}"),
                });
                break;
            }
        }

        if failure.is_none() {
            let expected_setup = vec![
                PortEvent(0x0038, expected_z180.cbr, PortDirection::W),
                PortEvent(0x003a, expected_z180.cbar, PortDirection::W),
                PortEvent(0x0039, expected_z180.bbr, PortDirection::W),
            ];
            let actual_setup = port_script.borrow().observed.clone();
            if actual_setup != expected_setup {
                failure = Some(Failure {
                    test: case.name.clone(),
                    field: "mmu.setup_ports".to_owned(),
                    expected: format!("{expected_setup:?}"),
                    actual: format!("{actual_setup:?}"),
                });
            }
        }

        if failure.is_none() {
            port_script.borrow_mut().observed.clear();
            for (index, probe) in case
                .mmu_probes
                .as_ref()
                .expect("generated MMU validation requires probes")
                .iter()
                .enumerate()
            {
                let actual_physical = cpu.mmu_translate(probe.logical);
                if actual_physical != probe.expected_physical {
                    failure = Some(Failure {
                        test: case.name.clone(),
                        field: format!("mmu_probes[{index}].expected_physical"),
                        expected: format!("{:05x}", probe.expected_physical),
                        actual: format!("{actual_physical:05x}"),
                    });
                    break;
                }

                let instruction_logical = probe.logical ^ 1;
                let instruction_physical = cpu.mmu_translate(instruction_logical);
                cpu.mem_poke(instruction_physical, 0x7e);
                cpu.mem_poke(probe.expected_physical, probe.value);
                cpu.set_reg(Reg::PC, instruction_logical);
                cpu.set_reg(Reg::HL, probe.logical);
                cpu.set_reg(Reg::AF, u16::from(!probe.value) << 8);
                let _ = cpu.step();
                let actual_value = cpu.reg(Reg::AF).to_be_bytes()[0];
                if actual_value != probe.value {
                    failure = Some(Failure {
                        test: case.name.clone(),
                        field: format!("mmu_probes[{index}].value"),
                        expected: format!("{:02x}", probe.value),
                        actual: format!("{actual_value:02x}"),
                    });
                    break;
                }
            }
        }

        if failure.is_none() {
            load_state(&mut cpu, &case.initial)
                .with_context(|| format!("failed to restore initial state for {}", case.name))?;
            failure = compare(&cpu, &case, ignore_r)
                .or_else(|| compare_ports(&port_script.borrow(), &case.name));
        }

        if let Some(failure) = failure {
            file_report.fail += 1;
            file_report.failures.push(failure);
        } else {
            file_report.pass += 1;
        }
    }

    Ok(file_report)
}

struct LdSabotage {
    address: u16,
    opcode: u8,
    writes_fetch_byte: bool,
}

impl LdSabotage {
    fn restore_fetch_byte(self, cpu: &mut Z180<ScriptedBus>) {
        if !self.writes_fetch_byte {
            cpu.mem_poke(u32::from(self.address), self.opcode);
        }
    }
}

fn inject_reversed_ld(cpu: &mut Z180<ScriptedBus>) -> Option<LdSabotage> {
    let address = cpu.reg(Reg::PC);
    let opcode = cpu.mem_peek(u32::from(address));
    let reversed = reversed_ld_opcode(opcode)?;
    let destination = (reversed >> 3) & 0x07;
    let writes_fetch_byte = destination == 0x06 && cpu.reg(Reg::HL) == address;
    cpu.mem_poke(u32::from(address), reversed);
    Some(LdSabotage {
        address,
        opcode,
        writes_fetch_byte,
    })
}

pub(super) fn reversed_ld_opcode(opcode: u8) -> Option<u8> {
    ((0x40..=0x7f).contains(&opcode) && opcode != 0x76)
        .then_some(0x40 | ((opcode & 0x07) << 3) | ((opcode >> 3) & 0x07))
}

pub(super) fn machine(
    expected_ports: Vec<PortEvent>,
    ram_size: u32,
) -> Result<(Z180<ScriptedBus>, Rc<RefCell<PortScript>>)> {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: ram_size,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    let script = Rc::new(RefCell::new(PortScript {
        expected: expected_ports,
        observed: Vec::new(),
    }));
    let bus = ScriptedBus {
        script: Rc::clone(&script),
    };
    let cpu = Z180::new(config, bus).context("flat SST machine configuration is invalid")?;
    Ok((cpu, script))
}

fn load_state(cpu: &mut Z180<ScriptedBus>, state: &TestState) -> Result<()> {
    for [address, value] in &state.ram {
        let value = u8::try_from(*value)
            .with_context(|| format!("RAM value {value} at {address:04x} exceeds one byte"))?;
        cpu.mem_poke(u32::from(*address), value);
    }

    cpu.set_reg(Reg::PC, state.pc);
    cpu.set_reg(Reg::SP, state.sp);
    cpu.set_reg(Reg::AF, pair(state.a, state.f));
    cpu.set_reg(Reg::BC, pair(state.b, state.c));
    cpu.set_reg(Reg::DE, pair(state.d, state.e));
    cpu.set_reg(Reg::HL, pair(state.h, state.l));
    cpu.set_reg(Reg::IX, state.ix);
    cpu.set_reg(Reg::IY, state.iy);
    cpu.set_reg(Reg::AF2, state.af2);
    cpu.set_reg(Reg::BC2, state.bc2);
    cpu.set_reg(Reg::DE2, state.de2);
    cpu.set_reg(Reg::HL2, state.hl2);
    cpu.set_reg(Reg::IR, pair(state.i, state.r));
    cpu.set_iff1(state.iff1 != 0);
    cpu.set_iff2(state.iff2 != 0);
    cpu.set_interrupt_mode(state.im);
    Ok(())
}

pub(super) const fn pair(high: u8, low: u8) -> u16 {
    u16::from_be_bytes([high, low])
}
