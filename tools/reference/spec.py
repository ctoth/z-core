"""Pure Z80180 transitions derived only from verified UM0050 facts.

Every authority comment names the corresponding row in
``docs/verification-log.md``.  This module deliberately imports neither
z-core nor an incumbent emulator.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from typing import Any

State = dict[str, Any]
PortEvent = list[int | str]

# Authority: verification-log row "The flag-table symbols mean ..." and the
# instruction-family rows that follow it. Bits 5 and 3 are masked by PLAN 3.1.4.
FLAG_S = 0x80
FLAG_Z = 0x40
FLAG_Y = 0x20
FLAG_H = 0x10
FLAG_X = 0x08
FLAG_PV = 0x04
FLAG_N = 0x02
FLAG_C = 0x01
DEFINED_FLAGS_MASK = 0xD7
OTIM_DEFINED_FLAGS_MASK = FLAG_Z | FLAG_N

# Authority: verification-log rows for the ITC register and MMU registers.
ITC_RESET = 0x01
ITC_TRAP = 0x80
ITC_UFO = 0x40
CBR_RESET = 0x00
BBR_RESET = 0x00
CBAR_RESET = 0xF0


@dataclass(frozen=True, slots=True)
class Instruction:
    """One Z180-added ED-page instruction form."""

    key: str
    mnemonic: str
    opcode: tuple[int, ...]
    operation: str
    register: str | None = None
    direction: int = 0
    repeat: bool = False

    @property
    def filename(self) -> str:
        return "".join(f"{byte:02x}" for byte in self.opcode[:2]) + ".json"

    @property
    def flags_mask(self) -> int:
        if self.operation == "otim" and not self.repeat:
            return OTIM_DEFINED_FLAGS_MASK
        return DEFINED_FLAGS_MASK


# Authority: verification-log row "Register codes 000-101 and 111 ..." and
# the IN0/OUT0/TST instruction-family rows. Code 110 is the IN0 flags-only
# form, TST (HL), and is absent from OUT0.
_REGISTERS_BY_G = ("b", "c", "d", "e", "h", "l", None, "a")

_in0 = tuple(
    Instruction(
        key=f"ed{g << 3:02x}",
        mnemonic="IN0 flags,(n)" if register is None else f"IN0 {register.upper()},(n)",
        opcode=(0xED, g << 3),
        operation="in0",
        register=register,
    )
    for g, register in enumerate(_REGISTERS_BY_G)
)

_out0 = tuple(
    Instruction(
        key=f"ed{(g << 3) | 1:02x}",
        mnemonic=f"OUT0 (n),{register.upper()}",
        opcode=(0xED, (g << 3) | 1),
        operation="out0",
        register=register,
    )
    for g, register in enumerate(_REGISTERS_BY_G)
    if register is not None
)

_tst = tuple(
    Instruction(
        key=f"ed{(g << 3) | 4:02x}",
        mnemonic="TST (HL)" if register is None else f"TST {register.upper()}",
        opcode=(0xED, (g << 3) | 4),
        operation="tst_hl" if register is None else "tst_reg",
        register=register,
    )
    for g, register in enumerate(_REGISTERS_BY_G)
)

# Authority: verification-log rows for MLT, TST, TSTIO, OTIM/OTDM, and SLP.
_mlt = tuple(
    Instruction(
        key=f"ed{second:02x}",
        mnemonic=f"MLT {pair.upper()}",
        opcode=(0xED, second),
        operation="mlt",
        register=pair,
    )
    for second, pair in ((0x4C, "bc"), (0x5C, "de"), (0x6C, "hl"), (0x7C, "sp"))
)

INSTRUCTIONS = (
    *_in0,
    *_out0,
    *_tst,
    Instruction("ed64", "TST n", (0xED, 0x64), "tst_imm"),
    Instruction("ed74", "TSTIO n", (0xED, 0x74), "tstio"),
    *_mlt,
    Instruction("ed83", "OTIM", (0xED, 0x83), "otim", direction=1),
    Instruction("ed93", "OTIMR", (0xED, 0x93), "otim", direction=1, repeat=True),
    Instruction("ed8b", "OTDM", (0xED, 0x8B), "otim", direction=-1),
    Instruction("ed9b", "OTDMR", (0xED, 0x9B), "otim", direction=-1, repeat=True),
    Instruction("ed76", "SLP", (0xED, 0x76), "slp"),
)

INSTRUCTION_BY_KEY = {instruction.key: instruction for instruction in INSTRUCTIONS}

# Authority: verification-log rows for CB SLL absence, the populated ED map,
# DD/FD substitution rules, and undefined-fetch TRAP behavior. These are all
# second-opcode undefined cases, for which UFO is unambiguously zero.
UNDEFINED_SECOND_OPCODES = (
    *((0xCB, second) for second in range(0x30, 0x38)),
    (0xED, 0x31),
    (0xDD, 0x24),
    (0xFD, 0x24),
)

# Table 48 only substitutes IX/IY for the documented Table 49 (HL) cells.
# Register-result DDCB/FDCB forms are undefined third-opcode fetches, for
# which UFO is one. One representative from each indexed page is sufficient
# to exercise the distinct stack/R/ITC transition.
UNDEFINED_THIRD_OPCODES = (
    (0xDD, 0xCB, 0x40),
    (0xFD, 0xCB, 0x40),
)

UNDEFINED_TRAP_OPCODES = (*UNDEFINED_SECOND_OPCODES, *UNDEFINED_THIRD_OPCODES)


def reset_z180_state() -> dict[str, int | bool]:
    """Return the verified reset defaults represented in Appendix C."""

    return {
        "itc": ITC_RESET,
        "cbr": CBR_RESET,
        "bbr": BBR_RESET,
        "cbar": CBAR_RESET,
        "sleeping": False,
    }


def ram_dict(state: State) -> dict[int, int]:
    """Expand a sparse SST RAM list into a byte-addressed mapping."""

    return {int(address): int(value) for address, value in state["ram"]}


def normalized_ram(memory: dict[int, int]) -> list[list[int]]:
    """Return deterministic sparse RAM entries sorted by 16-bit address."""

    return [[address & 0xFFFF, value & 0xFF] for address, value in sorted(memory.items())]


def _read_ram(state: State, address: int) -> int:
    try:
        return ram_dict(state)[address & 0xFFFF]
    except KeyError as error:
        raise ValueError(f"reference input omits RAM at {address & 0xFFFF:04x}") from error


def _increment_r(value: int, count: int = 2) -> int:
    # Authority: verification-log row "R bits 0-6 increment on every CPU
    # opcode-fetch (M1) cycle". ED and its second byte are two opcode fetches.
    return (value & 0x80) | ((value + count) & 0x7F)


def _finish_ed(final: State, initial: State, length: int) -> None:
    final["pc"] = (initial["pc"] + length) & 0xFFFF
    final["r"] = _increment_r(initial["r"])


def _parity(value: int) -> int:
    return FLAG_PV if (value & 0xFF).bit_count() % 2 == 0 else 0


def _logic_flags(result: int) -> int:
    result &= 0xFF
    return (
        (result & (FLAG_S | FLAG_Y | FLAG_X))
        | (FLAG_Z if result == 0 else 0)
        | FLAG_H
        | _parity(result)
    )


def _in_flags(result: int, previous: int) -> int:
    result &= 0xFF
    return (
        (result & (FLAG_S | FLAG_Y | FLAG_X))
        | (FLAG_Z if result == 0 else 0)
        | _parity(result)
        | (previous & FLAG_C)
    )


def _pair(state: State, name: str) -> int:
    if name == "sp":
        return int(state["sp"])
    return (int(state[name[0]]) << 8) | int(state[name[1]])


def _set_pair(state: State, name: str, value: int) -> None:
    value &= 0xFFFF
    if name == "sp":
        state["sp"] = value
    else:
        state[name[0]] = value >> 8
        state[name[1]] = value & 0xFF


def instruction_transition(
    initial: State,
    instruction: Instruction,
    *,
    port_value: int | None = None,
) -> tuple[State, list[PortEvent]]:
    """Apply one independent UM0050 transition to an Appendix C state."""

    final = deepcopy(initial)
    ports: list[PortEvent] = []
    operation = instruction.operation

    if operation == "in0":
        if port_value is None:
            raise ValueError("IN0 requires a deterministic port value")
        address = _read_ram(initial, initial["pc"] + 2)
        value = port_value & 0xFF
        ports.append([address, value, "r"])
        if instruction.register is not None:
            final[instruction.register] = value
        final["f"] = _in_flags(value, initial["f"])
        _finish_ed(final, initial, 3)
    elif operation == "out0":
        address = _read_ram(initial, initial["pc"] + 2)
        ports.append([address, initial[instruction.register], "w"])
        _finish_ed(final, initial, 3)
    elif operation in {"tst_reg", "tst_hl", "tst_imm"}:
        if operation == "tst_reg":
            operand = initial[instruction.register]
        elif operation == "tst_hl":
            operand = _read_ram(initial, _pair(initial, "hl"))
        else:
            operand = _read_ram(initial, initial["pc"] + 2)
        final["f"] = _logic_flags(initial["a"] & operand)
        _finish_ed(final, initial, 3 if operation == "tst_imm" else 2)
    elif operation == "tstio":
        if port_value is None:
            raise ValueError("TSTIO requires a deterministic port value")
        immediate = _read_ram(initial, initial["pc"] + 2)
        value = port_value & 0xFF
        ports.append([initial["c"], value, "r"])
        final["f"] = _logic_flags(value & immediate)
        _finish_ed(final, initial, 3)
    elif operation == "mlt":
        pair = _pair(initial, instruction.register)
        _set_pair(final, instruction.register, (pair >> 8) * (pair & 0xFF))
        _finish_ed(final, initial, 2)
    elif operation == "otim":
        if instruction.repeat and initial["b"] != 1:
            raise ValueError("Phase 1 repeat-opcode strategies require terminal B=1")
        address = _pair(initial, "hl")
        value = _read_ram(initial, address)
        ports.append([initial["c"], value, "w"])
        _set_pair(final, "hl", address + instruction.direction)
        final["c"] = (initial["c"] + instruction.direction) & 0xFF
        final["b"] = (initial["b"] - 1) & 0xFF
        n_flag = FLAG_N if value & 0x80 else 0
        if instruction.repeat:
            final["f"] = (
                (initial["f"] & (FLAG_Y | FLAG_X)) | FLAG_Z | FLAG_PV | n_flag
            )
        else:
            final["f"] = (
                (initial["f"] & ~OTIM_DEFINED_FLAGS_MASK)
                | (FLAG_Z if final["b"] == 0 else 0)
                | n_flag
            )
        _finish_ed(final, initial, 2)
    elif operation == "slp":
        final["z180"]["sleeping"] = True
        _finish_ed(final, initial, 2)
    else:
        raise ValueError(f"unknown reference operation: {operation}")

    return final, ports


def trap_transition(initial: State) -> State:
    """Apply the UM0050 undefined-fetch TRAP transition."""

    final = deepcopy(initial)
    pc = initial["pc"]
    memory = ram_dict(initial)
    third_opcode = memory[pc] in {0xDD, 0xFD} and memory[(pc + 1) & 0xFFFF] == 0xCB
    stacked_pc = (pc + (2 if third_opcode else 1)) & 0xFFFF
    sp_minus_one = (initial["sp"] - 1) & 0xFFFF
    sp_minus_two = (initial["sp"] - 2) & 0xFFFF
    memory[sp_minus_one] = stacked_pc >> 8
    memory[sp_minus_two] = stacked_pc & 0xFF
    final["ram"] = normalized_ram(memory)
    final["sp"] = sp_minus_two
    final["pc"] = 0
    final["r"] = _increment_r(initial["r"], 3 if third_opcode else 2)
    final["z180"]["itc"] = (initial["z180"]["itc"] | ITC_TRAP) & ~ITC_UFO
    if third_opcode:
        final["z180"]["itc"] |= ITC_UFO
    return final


def mmu_translate(logical: int, cbr: int, bbr: int, cbar: int) -> int:
    """Translate one logical address using the verified UM0050 page rule."""

    # Authority: verification-log rows for CBAR/CBR/BBR and Figures 27-30.
    logical &= 0xFFFF
    page = logical >> 12
    offset = logical & 0x0FFF
    ba = cbar & 0x0F
    ca = cbar >> 4
    if page < ba:
        base = 0
    elif page < ca:
        base = bbr
    else:
        base = cbr
    return ((((base + page) & 0xFF) << 12) | offset) & 0xFFFFF
