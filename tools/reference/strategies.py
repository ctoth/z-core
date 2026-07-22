"""Shared Hypothesis strategies for the UM0050 reference vocabulary."""

from __future__ import annotations

from copy import deepcopy

from hypothesis import strategies as st

from spec import (
    INSTRUCTIONS,
    UNDEFINED_TRAP_OPCODES,
    Instruction,
    instruction_transition,
    mmu_translate,
    normalized_ram,
    reset_z180_state,
)

# These primitive strategies are reused by corpus generation now and the
# mandatory Phase 8 differential properties later.
byte_values = st.integers(min_value=0, max_value=0xFF)
word_values = st.integers(min_value=0, max_value=0xFFFF)
flag_bytes = byte_values
memory_values = byte_values
# Authority: verification-log row "ICR bits 7-6 relocate the 64-byte internal
# I/O window ...". P1.5 excludes reset-state internal addresses 00h-3Fh.
external_port_addresses = st.integers(min_value=0x40, max_value=0xFF)
instruction_encodings = st.sampled_from(INSTRUCTIONS)


@st.composite
def cpu_states(draw: st.DrawFn) -> dict:
    """Generate a complete Appendix C CPU state with reset Z180 controls."""

    return {
        "pc": draw(st.integers(min_value=0x0100, max_value=0x1FF0)),
        "sp": draw(st.integers(min_value=0x8002, max_value=0xFFFE)),
        "a": draw(byte_values),
        "b": draw(byte_values),
        "c": draw(byte_values),
        "d": draw(byte_values),
        "e": draw(byte_values),
        "f": draw(flag_bytes),
        "h": draw(byte_values),
        "l": draw(byte_values),
        "i": draw(byte_values),
        "r": draw(byte_values),
        "ix": draw(word_values),
        "iy": draw(word_values),
        "af_": draw(word_values),
        "bc_": draw(word_values),
        "de_": draw(word_values),
        "hl_": draw(word_values),
        "iff1": draw(st.integers(min_value=0, max_value=1)),
        "iff2": draw(st.integers(min_value=0, max_value=1)),
        "im": draw(st.integers(min_value=0, max_value=2)),
        "ram": [],
        "z180": reset_z180_state(),
    }


def _set_pair(state: dict, pair: str, value: int) -> None:
    state[pair[0]] = value >> 8
    state[pair[1]] = value & 0xFF


@st.composite
def instruction_inputs(draw: st.DrawFn, instruction: Instruction) -> dict:
    """Generate one complete initial state and deterministic port input."""

    state = draw(cpu_states())
    memory: dict[int, int] = {}
    pc = state["pc"]
    for offset, byte in enumerate(instruction.opcode):
        memory[pc + offset] = byte

    port_value = None
    if instruction.operation in {"in0", "out0"}:
        memory[pc + 2] = draw(external_port_addresses)
    elif instruction.operation == "tst_imm":
        memory[pc + 2] = draw(memory_values)
    elif instruction.operation == "tstio":
        memory[pc + 2] = draw(memory_values)
        state["c"] = draw(external_port_addresses)

    if instruction.operation in {"in0", "tstio"}:
        port_value = draw(memory_values)

    if instruction.operation in {"tst_hl", "otim"}:
        address = draw(st.integers(min_value=0x4000, max_value=0x7FFF))
        _set_pair(state, "hl", address)
        memory[address] = draw(memory_values)

    if instruction.operation == "otim":
        state["c"] = draw(external_port_addresses)
        if instruction.repeat:
            # UM0050 gives complete terminal flag values. B=1 performs one
            # transfer and terminates, avoiding an invented intermediate state.
            state["b"] = 1

    state["ram"] = normalized_ram(memory)
    return {"initial": state, "port_value": port_value}


any_instruction_input = instruction_encodings.flatmap(
    lambda instruction: instruction_inputs(instruction).map(
        lambda generated: (instruction, generated)
    )
)


@st.composite
def sequence_inputs(draw: st.DrawFn) -> dict:
    """Generate up to 32 sequentially valid reference-modeled instructions."""

    initial = draw(cpu_states())
    initial["pc"] = 0x1000
    initial["ram"] = []
    state = deepcopy(initial)
    memory: dict[int, int] = {}
    steps = []
    count = draw(st.integers(min_value=1, max_value=32))

    for _ in range(count):
        hl = (state["h"] << 8) | state["l"]
        candidates = [
            instruction
            for instruction in INSTRUCTIONS
            if instruction.operation != "slp"
            and (not instruction.repeat or state["b"] == 1)
            and (
                instruction.operation not in {"tstio", "otim"}
                or state["c"] >= 0x40
            )
            and (
                instruction.operation not in {"tst_hl", "otim"}
                or not 0x1000 <= hl < 0x2000
            )
        ]
        instruction = draw(st.sampled_from(candidates))
        pc = state["pc"]
        updates: dict[int, int] = {}
        for offset, byte in enumerate(instruction.opcode):
            updates[pc + offset] = byte

        port_value = None
        if instruction.operation in {"in0", "out0"}:
            updates[pc + 2] = draw(external_port_addresses)
        elif instruction.operation in {"tst_imm", "tstio"}:
            updates[pc + 2] = draw(memory_values)

        if instruction.operation in {"in0", "tstio"}:
            port_value = draw(memory_values)

        if instruction.operation in {"tst_hl", "otim"}:
            updates[hl] = draw(memory_values)

        memory.update(updates)
        state["ram"] = normalized_ram(memory)
        final, _ports = instruction_transition(
            state,
            instruction,
            port_value=port_value,
        )
        steps.append(
            {
                "instruction": instruction,
                "memory_updates": normalized_ram(updates),
                "port_value": port_value,
            }
        )
        state = final

    return {"initial": initial, "steps": steps}


@st.composite
def trap_inputs(draw: st.DrawFn, opcode: tuple[int, ...]) -> dict:
    """Generate representative second- and third-opcode undefined-fetch states."""

    state = draw(cpu_states())
    if opcode not in UNDEFINED_TRAP_OPCODES:
        raise ValueError(f"opcode {opcode!r} is not a verified undefined form")
    memory = {
        state["pc"]: opcode[0],
        state["pc"] + 1: opcode[1],
    }
    if len(opcode) == 3:
        memory[state["pc"] + 2] = draw(byte_values)
        memory[state["pc"] + 3] = opcode[2]
    state["ram"] = normalized_ram(memory)
    return {"initial": state, "opcode": opcode}


any_trap_input = st.sampled_from(UNDEFINED_TRAP_OPCODES).flatmap(trap_inputs)


@st.composite
def mmu_register_triples(draw: st.DrawFn) -> tuple[int, int, int]:
    """Generate a valid CBAR partition with BA less than or equal to CA."""

    ba = draw(st.integers(min_value=0, max_value=0x0F))
    ca = draw(st.integers(min_value=ba, max_value=0x0F))
    cbr = draw(byte_values)
    bbr = draw(byte_values)
    return cbr, bbr, (ca << 4) | ba


@st.composite
def mmu_inputs(draw: st.DrawFn) -> dict:
    """Generate a randomized MMU program and one probe per logical page."""

    initial = draw(cpu_states())
    cbr, bbr, cbar = draw(mmu_register_triples())
    offsets = draw(
        st.lists(
            st.integers(min_value=0, max_value=0x0FFF),
            min_size=16,
            max_size=16,
        )
    )
    pattern_salt = draw(memory_values)
    probes = []
    for page, offset in enumerate(offsets):
        logical = (page << 12) | offset
        physical = mmu_translate(logical, cbr, bbr, cbar)
        value = (physical ^ (physical >> 8) ^ pattern_salt) & 0xFF
        probes.append(
            {
                "logical": logical,
                "expected_physical": physical,
                "value": value,
            }
        )

    final = {**initial, "z180": {**initial["z180"]}}
    final["z180"].update({"cbr": cbr, "bbr": bbr, "cbar": cbar})
    return {"initial": initial, "final": final, "mmu_probes": probes}
