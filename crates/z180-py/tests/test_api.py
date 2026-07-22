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


def test_machine_callbacks_are_keyword_only_and_internal_ram_bypasses_them():
    memory_reads = []
    memory_writes = []
    machine = z180.Machine(
        config_dict={"regions": [RAM_4K]},
        mem_read=lambda address: memory_reads.append(address) or 0xFF,
        mem_write=lambda address, value: memory_writes.append((address, value)),
    )
    ram = machine.ram(0)
    ram[:5] = b"\x3e\x5a\x32\x08\x00"  # LD A,5Ah; LD (0008h),A

    machine.step()
    machine.step()

    assert ram[8] == 0x5A
    assert memory_reads == []
    assert memory_writes == []
    with pytest.raises(TypeError, match="positional"):
        z180.Machine({"regions": [RAM_4K]}, lambda address: 0xFF)


def test_flash_aperture_uses_callbacks_and_omissions_keep_defaults():
    memory_reads = []
    memory_writes = []
    machine = z180.Machine(
        {
            "unmapped_read": 0xA5,
            "regions": [
                RAM_4K,
                {"base": 0x80000, "size": 0x80000, "kind": "external"},
            ],
        },
        mem_read=lambda address: memory_reads.append(address) or 0xC3,
        mem_write=lambda address, value: memory_writes.append((address, value)),
    )
    ram = machine.ram(0)
    ram[:18] = bytes(
        [
            0x3E,
            0xF1,  # LD A,F1h
            0xED,
            0x39,
            0x3A,  # OUT0 (CBAR),A: keep page zero common
            0x3E,
            0x7F,  # LD A,7Fh
            0xED,
            0x39,
            0x39,  # OUT0 (BBR),A: logical 1000h -> physical 80000h
            0x3E,
            0x5A,  # LD A,5Ah
            0x32,
            0x00,
            0x10,  # LD (1000h),A
            0x3A,
            0x00,
            0x10,  # LD A,(1000h)
        ]
    )

    for _ in range(7):
        machine.step()

    assert memory_writes == [(0x80000, 0x5A)]
    assert memory_reads == [0x80000]
    assert machine.reg(z180.Reg.AF) >> 8 == 0xC3

    disconnected = z180.Machine(
        {
            "unmapped_read": 0xA5,
            "regions": [
                RAM_4K,
                {"base": 0x80000, "size": 0x80000, "kind": "external"},
            ],
        }
    )
    disconnected.ram(0)[:13] = bytes(
        [
            0x3E,
            0xF1,
            0xED,
            0x39,
            0x3A,
            0x3E,
            0x7F,
            0xED,
            0x39,
            0x39,
            0x3A,
            0x00,
            0x10,
        ]
    )
    for _ in range(5):
        disconnected.step()
    assert disconnected.reg(z180.Reg.AF) >> 8 == 0xA5


def test_external_and_duplicate_internal_io_cycles_use_callbacks():
    io_reads = []
    io_writes = []
    machine = z180.Machine(
        {"regions": [RAM_4K]},
        io_read=lambda port: io_reads.append(port) or 0xA5,
        io_write=lambda port, value: io_writes.append((port, value)),
    )
    machine.ram(0)[:12] = bytes(
        [
            0xED,
            0x39,
            0x40,  # OUT0 (40h),A: external
            0xED,
            0x39,
            0x04,  # OUT0 (STAT0),A: internal plus duplicate cycle
            0xED,
            0x38,
            0x40,  # IN0 A,(40h): external
            0xED,
            0x38,
            0x04,  # IN0 A,(STAT0): internal plus duplicate cycle
        ]
    )
    machine.set_reg(z180.Reg.AF, 0x5A00)

    for _ in range(4):
        machine.step()

    assert io_writes == [(0x40, 0x5A), (0x04, 0x5A)]
    assert io_reads == [0x40, 0x04]


def test_callback_error_propagates_once_and_does_not_remain_pending():
    calls = 0

    def mem_read(address):
        nonlocal calls
        calls += 1
        if calls == 1:
            raise RuntimeError(f"read failed at {address:#x}")
        return 0x00

    machine = z180.Machine(
        {"regions": [{"base": 0, "size": 0x1000, "kind": "external"}]},
        mem_read=mem_read,
    )

    with pytest.raises(RuntimeError, match="read failed at 0x0"):
        machine.step()
    assert machine.step() > 0


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
