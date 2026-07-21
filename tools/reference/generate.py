"""Generate the checked-in Z180-specific single-step corpus."""

from __future__ import annotations

import argparse
import json
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    generate_corpus(args.out)


if __name__ == "__main__":
    main()
