"""Mandatory Hypothesis differential properties from PLAN P8.6."""

from __future__ import annotations

from typing import Any

from hypothesis import given

from spec import (
    Instruction,
    instruction_transition,
    mmu_translate,
    normalized_ram,
    ram_dict,
    trap_transition,
)
from strategies import (
    any_instruction_input,
    any_trap_input,
    mmu_inputs,
    sequence_inputs,
)
from z180 import Machine, Reg, WatchKind


RAM_64K = {"base": 0, "size": 0x10000, "kind": "ram"}
RAM_1M = {"base": 0, "size": 0x100000, "kind": "ram"}


def pair(state: dict[str, Any], name: str) -> int:
    return (state[name[0]] << 8) | state[name[1]]


def load_cpu_state(machine: Machine, state: dict[str, Any]) -> None:
    machine.set_reg(Reg.PC, state["pc"])
    machine.set_reg(Reg.SP, state["sp"])
    machine.set_reg(Reg.AF, pair(state, "af"))
    machine.set_reg(Reg.BC, pair(state, "bc"))
    machine.set_reg(Reg.DE, pair(state, "de"))
    machine.set_reg(Reg.HL, pair(state, "hl"))
    machine.set_reg(Reg.IX, state["ix"])
    machine.set_reg(Reg.IY, state["iy"])
    machine.set_reg(Reg.AF2, state["af_"])
    machine.set_reg(Reg.BC2, state["bc_"])
    machine.set_reg(Reg.DE2, state["de_"])
    machine.set_reg(Reg.HL2, state["hl_"])
    machine.set_reg(Reg.IR, (state["i"] << 8) | state["r"])
    machine.set_iff1(bool(state["iff1"]))
    machine.set_iff2(bool(state["iff2"]))
    machine.set_interrupt_mode(state["im"])
    for address, value in state["ram"]:
        machine.mem_poke(address, value)


def make_machine(
    state: dict[str, Any],
    port_value: int | None,
) -> tuple[Machine, list[list[int | str]], list[int]]:
    io_events: list[list[int | str]] = []
    current_port_value = [0xFF if port_value is None else port_value]

    def io_read(port: int) -> int:
        value = current_port_value[0]
        io_events.append([port, value, "r"])
        return value

    def io_write(port: int, value: int) -> None:
        io_events.append([port, value, "w"])

    machine = Machine(
        config_dict={"regions": [RAM_64K], "event_capacity": 32},
        io_read=io_read,
        io_write=io_write,
    )
    load_cpu_state(machine, state)
    machine.add_mem_watch(0, 0x10000, WatchKind.Both)
    return machine, io_events, current_port_value


def assert_machine_state(
    machine: Machine,
    expected: dict[str, Any],
    flags_mask: int,
) -> None:
    expected_pairs = (
        (Reg.PC, expected["pc"]),
        (Reg.SP, expected["sp"]),
        (Reg.BC, pair(expected, "bc")),
        (Reg.DE, pair(expected, "de")),
        (Reg.HL, pair(expected, "hl")),
        (Reg.IX, expected["ix"]),
        (Reg.IY, expected["iy"]),
        (Reg.AF2, expected["af_"]),
        (Reg.BC2, expected["bc_"]),
        (Reg.DE2, expected["de_"]),
        (Reg.HL2, expected["hl_"]),
        (Reg.IR, (expected["i"] << 8) | expected["r"]),
    )
    for register, value in expected_pairs:
        assert machine.reg(register) == value

    actual_af = machine.reg(Reg.AF)
    assert actual_af >> 8 == expected["a"]
    assert actual_af & flags_mask == expected["f"] & flags_mask
    assert machine.iff1() is bool(expected["iff1"])
    assert machine.iff2() is bool(expected["iff2"])
    assert machine.interrupt_mode() == expected["im"]
    assert machine.io_reg_peek(0x34) == expected["z180"]["itc"]
    assert machine.io_reg_peek(0x38) == expected["z180"]["cbr"]
    assert machine.io_reg_peek(0x39) == expected["z180"]["bbr"]
    assert machine.io_reg_peek(0x3A) == expected["z180"]["cbar"]
    assert machine.sleeping() is expected["z180"]["sleeping"]
    for address, value in expected["ram"]:
        assert machine.mem_peek(address) == value


def expected_instruction_memory_events(
    initial: dict[str, Any], instruction: Instruction
) -> list[tuple[str, int, int]]:
    memory = ram_dict(initial)
    length = (
        3
        if instruction.operation in {"in0", "out0", "tst_imm", "tstio"}
        else 2
    )
    events = [
        ("mem_read", (initial["pc"] + offset) & 0xFFFF, memory[initial["pc"] + offset])
        for offset in range(length)
    ]
    if instruction.operation in {"tst_hl", "otim"}:
        address = pair(initial, "hl")
        events.append(("mem_read", address, memory[address]))
    return events


def observed_memory_events(machine: Machine) -> list[tuple[str, int, int]]:
    return [
        (event["kind"], event["phys"], event["value"])
        for event in machine.drain_events()
        if event["kind"] in {"mem_read", "mem_write"}
    ]


@given(any_instruction_input)
def test_property_a_single_z180_instruction(sample) -> None:
    instruction, generated = sample
    expected, expected_io = instruction_transition(
        generated["initial"],
        instruction,
        port_value=generated["port_value"],
    )
    machine, observed_io, _current_port_value = make_machine(
        generated["initial"], generated["port_value"]
    )

    machine.step()

    assert_machine_state(machine, expected, instruction.flags_mask)
    assert observed_memory_events(machine) == expected_instruction_memory_events(
        generated["initial"], instruction
    )
    assert observed_io == expected_io


@given(sequence_inputs())
def test_property_b_short_sequences(sample) -> None:
    machine, observed_io, current_port_value = make_machine(sample["initial"], None)
    reference_state = sample["initial"]

    for step in sample["steps"]:
        instruction = step["instruction"]
        reference_memory = ram_dict(reference_state)
        reference_memory.update(dict(step["memory_updates"]))
        reference_state["ram"] = normalized_ram(reference_memory)
        expected, expected_io = instruction_transition(
            reference_state,
            instruction,
            port_value=step["port_value"],
        )
        for address, value in step["memory_updates"]:
            machine.mem_poke(address, value)
        observed_io.clear()
        current_port_value[0] = (
            0xFF if step["port_value"] is None else step["port_value"]
        )

        machine.step()

        assert_machine_state(machine, expected, instruction.flags_mask)
        assert observed_memory_events(machine) == expected_instruction_memory_events(
            reference_state, instruction
        )
        assert observed_io == expected_io

        actual_flags = machine.reg(Reg.AF) & 0xFF
        expected["f"] = (expected["f"] & instruction.flags_mask) | (
            actual_flags & ~instruction.flags_mask & 0xFF
        )
        reference_state = expected


def assert_trap_case(sample: dict[str, Any]) -> None:
    expected = trap_transition(sample["initial"])
    machine, observed_io, _current_port_value = make_machine(sample["initial"], None)
    initial = sample["initial"]
    memory = ram_dict(initial)
    stacked_pc = (initial["pc"] + 2) & 0xFFFF
    expected_events = [
        ("mem_read", initial["pc"], memory[initial["pc"]]),
        ("mem_read", (initial["pc"] + 1) & 0xFFFF, memory[initial["pc"] + 1]),
        ("mem_write", (initial["sp"] - 1) & 0xFFFF, stacked_pc >> 8),
        ("mem_write", (initial["sp"] - 2) & 0xFFFF, stacked_pc & 0xFF),
    ]

    machine.step()

    assert_machine_state(machine, expected, 0xFF)
    assert sorted(observed_memory_events(machine)) == sorted(expected_events)
    assert observed_io == []


def configure_mmu(machine: Machine, cbr: int, bbr: int, cbar: int) -> None:
    program = bytes(
        [
            0x3E,
            0xF1,
            0xED,
            0x39,
            0x3A,
            0x3E,
            cbr,
            0xED,
            0x39,
            0x38,
            0x3E,
            bbr,
            0xED,
            0x39,
            0x39,
            0x3E,
            cbar,
            0xED,
            0x39,
            0x3A,
        ]
    )
    machine.ram(0)[: len(program)] = program
    for _ in range(8):
        machine.step()


def assert_mmu_case(sample: dict[str, Any]) -> None:
    expected_z180 = sample["final"]["z180"]
    cbr = expected_z180["cbr"]
    bbr = expected_z180["bbr"]
    cbar = expected_z180["cbar"]
    machine = Machine(config_dict={"regions": [RAM_1M]})
    configure_mmu(machine, cbr, bbr, cbar)
    assert machine.io_reg_peek(0x38) == cbr
    assert machine.io_reg_peek(0x39) == bbr
    assert machine.io_reg_peek(0x3A) == cbar

    for probe in sample["mmu_probes"]:
        logical = probe["logical"]
        expected_physical = probe["expected_physical"]
        assert mmu_translate(logical, cbr, bbr, cbar) == expected_physical
        assert machine.mmu_translate(logical) == expected_physical

        page = logical >> 12
        probe_offset = logical & 0x0FFF
        code_offset = 4 if probe_offset <= 2 else 0
        code_logical = (page << 12) | code_offset
        code = bytes((0x3A, logical & 0xFF, logical >> 8))
        machine.mem_poke(expected_physical, probe["value"])
        for offset, byte in enumerate(code):
            code_physical = machine.mmu_translate(code_logical + offset)
            machine.mem_poke(code_physical, byte)
        machine.set_reg(Reg.AF, 0)
        machine.set_reg(Reg.PC, code_logical)

        machine.step()

        assert machine.reg(Reg.AF) >> 8 == probe["value"]


@given(trap=any_trap_input, mmu=mmu_inputs())
def test_property_c_trap_and_mmu(trap, mmu) -> None:
    assert_trap_case(trap)
    assert_mmu_case(mmu)
