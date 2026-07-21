"""Generate the checked-in Z180-specific single-step corpus."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path
from typing import Any

from hypothesis import HealthCheck, Phase, given, seed, settings
from hypothesis.strategies import SearchStrategy

from spec import (
    DEFINED_FLAGS_MASK,
    INSTRUCTIONS,
    UNDEFINED_SECOND_OPCODES,
    instruction_transition,
    trap_transition,
)
from strategies import instruction_inputs, mmu_inputs, trap_inputs

OPCODE_CASES = 200
TRAP_CASES = 50
MMU_CASES = 200
BASE_SEED = 0x5A18_0000

STATE_FIELDS = {
    "pc",
    "sp",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "h",
    "l",
    "i",
    "r",
    "ix",
    "iy",
    "af_",
    "bc_",
    "de_",
    "hl_",
    "iff1",
    "iff2",
    "im",
    "ram",
    "z180",
}
BYTE_FIELDS = {"a", "b", "c", "d", "e", "f", "h", "l", "i", "r"}
WORD_FIELDS = {"pc", "sp", "ix", "iy", "af_", "bc_", "de_", "hl_"}
Z180_FIELDS = {"itc", "cbr", "bbr", "cbar", "sleeping"}
COMMON_FIELDS = {
    "name",
    "kind",
    "seed",
    "flags_mask",
    "disputed",
    "dispute_note",
    "ports",
    "initial",
    "final",
}


def collect_examples(
    strategy: SearchStrategy,
    *,
    count: int,
    seed_value: int,
) -> list[Any]:
    """Collect exactly ``count`` generation-phase Hypothesis examples."""

    examples: list[Any] = []

    @seed(seed_value)
    @settings(
        max_examples=count,
        derandomize=True,
        database=None,
        deadline=None,
        phases=(Phase.generate,),
        suppress_health_check=(HealthCheck.too_slow,),
    )
    @given(strategy)
    def collect(example: Any) -> None:
        examples.append(example)

    collect()
    if len(examples) != count:
        raise RuntimeError(
            f"seed {seed_value} produced {len(examples)} examples, expected {count}"
        )
    return examples


def _common_case(kind: str, seed_value: int, name: str) -> dict[str, Any]:
    return {
        "name": name,
        "kind": kind,
        "seed": seed_value,
        "flags_mask": DEFINED_FLAGS_MASK,
        "disputed": False,
        "dispute_note": "",
        "ports": [],
    }


def _instruction_cases(instruction, seed_value: int) -> list[dict[str, Any]]:
    generated = collect_examples(
        instruction_inputs(instruction),
        count=OPCODE_CASES,
        seed_value=seed_value,
    )
    cases = []
    for index, inputs in enumerate(generated):
        final, ports = instruction_transition(
            inputs["initial"],
            instruction,
            port_value=inputs["port_value"],
        )
        case = _common_case(
            "instruction",
            seed_value,
            f"{instruction.key.upper()} {index:04X}",
        )
        case.update(
            {
                "flags_mask": instruction.flags_mask,
                "initial": inputs["initial"],
                "final": final,
                "ports": ports,
            }
        )
        cases.append(case)
    return cases


def _trap_cases(seed_value: int) -> list[dict[str, Any]]:
    cases = []
    minimum, remainder = divmod(TRAP_CASES, len(UNDEFINED_SECOND_OPCODES))
    for opcode_index, opcode in enumerate(UNDEFINED_SECOND_OPCODES):
        subgroup_count = minimum + (1 if opcode_index < remainder else 0)
        subgroup_seed = seed_value + opcode_index
        generated = collect_examples(
            trap_inputs(opcode),
            count=subgroup_count,
            seed_value=subgroup_seed,
        )
        for inputs in generated:
            index = len(cases)
            case = _common_case(
                "trap",
                subgroup_seed,
                f"TRAP {opcode[0]:02X}{opcode[1]:02X} {index:04X}",
            )
            case.update(
                {
                    "initial": inputs["initial"],
                    "final": trap_transition(inputs["initial"]),
                }
            )
            cases.append(case)
    if len(cases) != TRAP_CASES:
        raise RuntimeError(f"generated {len(cases)} TRAP cases, expected {TRAP_CASES}")
    return cases


def _mmu_cases(seed_value: int) -> list[dict[str, Any]]:
    generated = collect_examples(
        mmu_inputs(),
        count=MMU_CASES,
        seed_value=seed_value,
    )
    cases = []
    for index, inputs in enumerate(generated):
        case = _common_case("mmu", seed_value, f"MMU {index:04X}")
        case.update(inputs)
        cases.append(case)
    return cases


def _write_cases(path: Path, cases: list[dict[str, Any]]) -> None:
    encoded = json.dumps(cases, sort_keys=True, separators=(",", ":")) + "\n"
    path.write_text(encoded, encoding="utf-8", newline="\n")


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _integer_in(value: Any, minimum: int, maximum: int) -> bool:
    return type(value) is int and minimum <= value <= maximum


def _validate_state(state: Any, location: str) -> None:
    _require(isinstance(state, dict), f"{location} must be an object")
    _require(set(state) == STATE_FIELDS, f"{location} has wrong state fields")
    for field in BYTE_FIELDS:
        _require(_integer_in(state[field], 0, 0xFF), f"{location}.{field} is not a byte")
    for field in WORD_FIELDS:
        _require(
            _integer_in(state[field], 0, 0xFFFF),
            f"{location}.{field} is not a word",
        )
    for field in ("iff1", "iff2"):
        _require(
            _integer_in(state[field], 0, 1),
            f"{location}.{field} is not boolean-valued",
        )
    _require(
        type(state["im"]) is int and state["im"] in (0, 1, 2),
        f"{location}.im is invalid",
    )

    ram = state["ram"]
    _require(isinstance(ram, list), f"{location}.ram must be an array")
    addresses = []
    for index, entry in enumerate(ram):
        _require(
            isinstance(entry, list) and len(entry) == 2,
            f"{location}.ram[{index}] must be [address,value]",
        )
        _require(
            _integer_in(entry[0], 0, 0xFFFF),
            f"{location}.ram[{index}] address is invalid",
        )
        _require(
            _integer_in(entry[1], 0, 0xFF),
            f"{location}.ram[{index}] value is invalid",
        )
        addresses.append(entry[0])
    _require(addresses == sorted(set(addresses)), f"{location}.ram is not canonical")

    z180 = state["z180"]
    _require(isinstance(z180, dict), f"{location}.z180 must be an object")
    _require(set(z180) == Z180_FIELDS, f"{location}.z180 has wrong fields")
    for field in ("itc", "cbr", "bbr", "cbar"):
        _require(
            _integer_in(z180[field], 0, 0xFF),
            f"{location}.z180.{field} is not a byte",
        )
    _require(type(z180["sleeping"]) is bool, f"{location}.z180.sleeping is not bool")


def _validate_common_case(case: Any, location: str, expected_kind: str) -> None:
    _require(isinstance(case, dict), f"{location} must be an object")
    expected_fields = COMMON_FIELDS | ({"mmu_probes"} if expected_kind == "mmu" else set())
    _require(set(case) == expected_fields, f"{location} has wrong top-level fields")
    _require(isinstance(case["name"], str) and case["name"], f"{location}.name is invalid")
    _require(case["kind"] == expected_kind, f"{location}.kind is not {expected_kind}")
    _require(type(case["seed"]) is int and case["seed"] >= 0, f"{location}.seed is invalid")
    _require(
        _integer_in(case["flags_mask"], 0, 0xFF) and not case["flags_mask"] & 0x28,
        f"{location}.flags_mask is invalid",
    )
    _require(type(case["disputed"]) is bool, f"{location}.disputed is not bool")
    _require(isinstance(case["dispute_note"], str), f"{location}.dispute_note is invalid")
    _require(
        not case["disputed"] or bool(case["dispute_note"]),
        f"{location} is disputed without a note",
    )
    _validate_state(case["initial"], f"{location}.initial")
    _validate_state(case["final"], f"{location}.final")

    ports = case["ports"]
    _require(isinstance(ports, list), f"{location}.ports must be an array")
    for index, event in enumerate(ports):
        _require(
            isinstance(event, list) and len(event) == 3,
            f"{location}.ports[{index}] must have three fields",
        )
        _require(
            _integer_in(event[0], 0, 0xFFFF),
            f"{location}.ports[{index}] address is invalid",
        )
        _require(
            _integer_in(event[1], 0, 0xFF),
            f"{location}.ports[{index}] value is invalid",
        )
        _require(event[2] in ("r", "w"), f"{location}.ports[{index}] direction is invalid")


def _validate_mmu_probes(case: dict[str, Any], location: str) -> None:
    probes = case["mmu_probes"]
    _require(isinstance(probes, list) and len(probes) == 16, f"{location} needs 16 probes")
    pages = []
    for index, probe in enumerate(probes):
        probe_location = f"{location}.mmu_probes[{index}]"
        _require(
            isinstance(probe, dict)
            and set(probe) == {"logical", "expected_physical", "value"},
            f"{probe_location} has wrong fields",
        )
        _require(
            _integer_in(probe["logical"], 0, 0xFFFF),
            f"{probe_location}.logical is invalid",
        )
        _require(
            _integer_in(probe["expected_physical"], 0, 0xFFFFF),
            f"{probe_location}.expected_physical is invalid",
        )
        _require(
            _integer_in(probe["value"], 0, 0xFF),
            f"{probe_location}.value is invalid",
        )
        pages.append(probe["logical"] >> 12)
    _require(pages == list(range(16)), f"{location} does not probe pages in order")


def _relative_files(root: Path) -> list[Path]:
    return sorted(path.relative_to(root) for path in root.rglob("*") if path.is_file())


def validate_corpus(root: Path) -> None:
    """Validate every generated case against Appendix C and P1.5 counts."""

    expected = {instruction.filename: ("instruction", OPCODE_CASES, instruction) for instruction in INSTRUCTIONS}
    expected["trap.json"] = ("trap", TRAP_CASES, None)
    expected["mmu.json"] = ("mmu", MMU_CASES, None)
    actual_files = _relative_files(root)
    _require(actual_files == [Path(name) for name in sorted(expected)], "corpus file set is wrong")

    trap_opcodes = set()
    for filename, (kind, minimum_count, instruction) in expected.items():
        path = root / filename
        cases = json.loads(path.read_text(encoding="utf-8"))
        _require(isinstance(cases, list), f"{filename} must contain an array")
        _require(len(cases) >= minimum_count, f"{filename} has too few cases")
        names = []
        for index, case in enumerate(cases):
            location = f"{filename}[{index}]"
            _validate_common_case(case, location, kind)
            names.append(case["name"])
            if kind == "instruction":
                _require(
                    case["flags_mask"] == instruction.flags_mask,
                    f"{location}.flags_mask disagrees with its instruction",
                )
                memory = {address: value for address, value in case["initial"]["ram"]}
                pc = case["initial"]["pc"]
                actual_opcode = tuple(memory[(pc + offset) & 0xFFFF] for offset in range(2))
                _require(actual_opcode == instruction.opcode[:2], f"{location} has wrong opcode")
            elif kind == "trap":
                _require(
                    case["flags_mask"] == DEFINED_FLAGS_MASK,
                    f"{location}.flags_mask is not the documented default",
                )
                memory = {address: value for address, value in case["initial"]["ram"]}
                pc = case["initial"]["pc"]
                opcode = (memory[pc], memory[(pc + 1) & 0xFFFF])
                _require(opcode in UNDEFINED_SECOND_OPCODES, f"{location} is not a verified TRAP")
                trap_opcodes.add(opcode)
            else:
                _require(
                    case["flags_mask"] == DEFINED_FLAGS_MASK,
                    f"{location}.flags_mask is not the documented default",
                )
                _validate_mmu_probes(case, location)
        _require(len(names) == len(set(names)), f"{filename} has duplicate case names")
    _require(
        trap_opcodes == set(UNDEFINED_SECOND_OPCODES),
        "TRAP corpus does not cover every representative undefined form",
    )


def compare_corpora(left: Path, right: Path) -> None:
    """Require byte-identical files at every relative corpus path."""

    left_files = _relative_files(left)
    right_files = _relative_files(right)
    _require(left_files == right_files, "corpus relative file trees differ")
    for relative in left_files:
        _require(
            (left / relative).read_bytes() == (right / relative).read_bytes(),
            f"corpus bytes differ at {relative}",
        )


def generate_corpus(output: Path) -> None:
    """Generate every P1.5 corpus file beneath ``output``."""

    output.mkdir(parents=True, exist_ok=True)
    for index, instruction in enumerate(INSTRUCTIONS):
        seed_value = BASE_SEED + index
        _write_cases(
            output / instruction.filename,
            _instruction_cases(instruction, seed_value),
        )
    _write_cases(output / "trap.json", _trap_cases(BASE_SEED + 0x1000))
    _write_cases(output / "mmu.json", _mmu_cases(BASE_SEED + 0x2000))


def check_corpus(expected: Path) -> None:
    """Regenerate, validate, and compare against a checked-in corpus."""

    validate_corpus(expected)
    with tempfile.TemporaryDirectory(prefix="z180-reference-") as temporary:
        regenerated = Path(temporary) / "corpus"
        generate_corpus(regenerated)
        validate_corpus(regenerated)
        compare_corpora(expected, regenerated)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    output_mode = parser.add_mutually_exclusive_group(required=True)
    output_mode.add_argument("--out", type=Path)
    output_mode.add_argument("--check", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.out is not None:
        generate_corpus(args.out)
    else:
        check_corpus(args.check)


if __name__ == "__main__":
    main()
