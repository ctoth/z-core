import gc

import pytest

import z180


RAM_4K = {"base": 0, "size": 0x1000, "kind": "ram"}


def machine_with_ram(**overrides):
    config = {"regions": [RAM_4K]}
    config.update(overrides)
    return z180.Machine(config)


def test_public_surface_covers_core_api():
    expected = {
        "reset",
        "step",
        "run",
        "cycle_count",
        "halted",
        "sleeping",
        "reg",
        "set_reg",
        "instruction_pc",
        "iff1",
        "set_iff1",
        "iff2",
        "set_iff2",
        "interrupt_mode",
        "set_interrupt_mode",
        "set_irq",
        "set_nmi",
        "set_dreq",
        "io_reg_peek",
        "mmu_translate",
        "asci_rx_push",
        "asci_tx_pop",
        "csio_rx_push",
        "csio_tx_pop",
        "set_asci_cts",
        "set_asci_dcd",
        "mem_peek",
        "mem_poke",
        "remap",
        "set_ext_mapper",
        "ram_regions",
        "ram",
        "add_mem_watch",
        "remove_mem_watch",
        "set_io_trace",
        "set_irq_trace",
        "set_pc_watch",
        "pc_watch_hits",
        "drain_events",
        "events_lost",
        "clear_events_lost",
        "set_insn_trace",
        "drain_insn_trace",
        "save_state",
        "load_state",
        "is_instruction_implemented",
    }
    assert expected <= set(dir(z180.Machine))
    assert z180.Reg.PC != z180.Reg.SP
    assert z180.IrqLine.Int0 != z180.IrqLine.Int1
    assert z180.WatchKind.Read != z180.WatchKind.Write


def test_config_is_strict_and_defaults_are_usable():
    assert z180.Machine().mem_peek(0) == 0xFF

    machine = z180.Machine(
        {
            "clock_hz": 18_432_000,
            "phys_addr_bits": 20,
            "unmapped_read": 0xA5,
            "variant": "Z8S180",
            "regions": [{"base": 0, "size": 0x1000, "kind": "external"}],
            "event_capacity": 8,
        }
    )
    assert machine.mem_peek(0) == 0xA5

    with pytest.raises(ValueError, match="unknown config field"):
        z180.Machine({"clock": 1})
    with pytest.raises(ValueError, match="variant"):
        z180.Machine({"variant": "z80"})
    with pytest.raises(KeyError):
        z180.Machine({"regions": [{"base": 0, "size": 0x1000}]})
    with pytest.raises(ValueError, match="ROM region requires data"):
        z180.Machine(
            {"regions": [{"base": 0, "size": 0x1000, "kind": "rom"}]}
        )
    with pytest.raises(ValueError, match="does not accept data"):
        z180.Machine(
            {
                "regions": [
                    {"base": 0, "size": 0x1000, "kind": "ram", "data": b""}
                ]
            }
        )


def test_register_lifecycle_interrupt_and_queue_methods():
    machine = machine_with_ram()
    machine.set_reg(z180.Reg.PC, 0x1234)
    machine.set_reg(z180.Reg.AF2, 0xABCD)
    assert machine.reg(z180.Reg.PC) == 0x1234
    assert machine.reg(z180.Reg.AF2) == 0xABCD

    machine.set_iff1(True)
    machine.set_iff2(True)
    machine.set_interrupt_mode(2)
    assert machine.iff1() is True
    assert machine.iff2() is True
    assert machine.interrupt_mode() == 2
    machine.set_irq(z180.IrqLine.Int0, False)
    machine.set_nmi(False)
    machine.set_dreq(0, False)
    machine.set_asci_cts(0, True)
    machine.set_asci_dcd(1, True)

    assert machine.asci_rx_push(0, 0x41) is False
    assert machine.asci_tx_pop(0) is None
    assert machine.csio_rx_push(0x42) is False
    assert machine.csio_tx_pop() is None
    with pytest.raises(ValueError, match="channel"):
        machine.asci_rx_push(2, 0)

    machine.reset()
    assert machine.cycle_count() == 0
    assert machine.halted() is False
    assert machine.sleeping() is False


def test_ram_memoryview_is_writable_zero_copy_and_guards_storage_replacement():
    machine = machine_with_ram()
    assert machine.ram_regions() == [(0, 0x1000)]

    view = machine.ram(0)
    assert isinstance(view, memoryview)
    assert view.readonly is False
    assert view.format == "B"
    assert len(view) == 0x1000

    view[7] = 0x81
    assert machine.mem_peek(7) == 0x81
    machine.mem_poke(8, 0x42)
    assert view[8] == 0x42

    state = machine.save_state()
    with pytest.raises(BufferError, match="remap"):
        machine.remap(0, 0x1000, "ram")
    with pytest.raises(BufferError, match="load state"):
        machine.load_state(state)

    view.release()
    del view
    gc.collect()
    machine.remap(0, 0x1000, "rom", bytes(0x1000))
    assert machine.ram_regions() == []
    with pytest.raises(KeyError, match="no RAM region"):
        machine.ram(0)


def test_external_mapper_is_complete_and_failed_sampling_is_atomic():
    machine = z180.Machine(
        {"regions": [{"base": 0x1000, "size": 0x1000, "kind": "ram"}]}
    )
    machine.mem_poke(0x1000, 0x00)
    machine.set_ext_mapper(lambda address: address ^ 0x1000)
    assert machine.mmu_translate(0) == 0

    def fail_during_sampling(address):
        if address == 17:
            raise RuntimeError("mapper failed")
        return address

    with pytest.raises(RuntimeError, match="mapper failed"):
        machine.set_ext_mapper(fail_during_sampling)

    assert machine.step() > 0
    assert machine.reg(z180.Reg.PC) == 1
    machine.set_ext_mapper(None)


def test_debug_events_trace_and_save_state_are_python_values():
    machine = machine_with_ram(event_capacity=8)
    machine.mem_poke(0, 0x00)
    watch = machine.add_mem_watch(0, 1, z180.WatchKind.Read)
    assert repr(watch) == "WatchId(<opaque>)"
    machine.set_pc_watch(0)
    machine.set_insn_trace(4)

    assert machine.step() > 0
    assert machine.instruction_pc() == 0
    assert machine.pc_watch_hits() == 1
    events = machine.drain_events()
    assert events == [
        {"kind": "mem_read", "cycle": 0, "pc": 0, "phys": 0, "value": 0}
    ]
    traces = machine.drain_insn_trace()
    assert traces == [{"cycle": 0, "pc": 0, "phys_pc": 0, "bytes": b"\x00", "len": 1}]

    machine.remove_mem_watch(watch)
    machine.set_io_trace(True)
    machine.set_irq_trace(True)
    assert machine.events_lost() is False
    machine.clear_events_lost()

    state = machine.save_state()
    assert isinstance(state, bytes)
    machine.set_reg(z180.Reg.PC, 0x2222)
    machine.load_state(state)
    assert machine.reg(z180.Reg.PC) == 1


def test_instruction_implementation_query_accepts_bytes():
    assert z180.Machine.is_instruction_implemented(b"\x00") is True
    assert z180.Machine.is_instruction_implemented(b"") is False
