"""P1.5b self-consistency and schema gates for the reference generator."""

from pathlib import Path

from hypothesis import HealthCheck, given, settings

from generate import compare_corpora, generate_corpus, validate_corpus
from spec import instruction_transition, ram_dict, trap_transition
from strategies import any_instruction_input

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
CHECKED_IN_CORPUS = REPOSITORY_ROOT / "tests" / "z180-sst"


@settings(
    max_examples=1000,
    derandomize=True,
    database=None,
    deadline=None,
    suppress_health_check=(HealthCheck.too_slow,),
)
@given(any_instruction_input)
def test_reference_transition_is_deterministic(sample) -> None:
    instruction, generated = sample
    first = instruction_transition(
        generated["initial"],
        instruction,
        port_value=generated["port_value"],
    )
    second = instruction_transition(
        generated["initial"],
        instruction,
        port_value=generated["port_value"],
    )
    assert first == second


def test_second_opcode_trap_stacks_the_undefined_fetch_address() -> None:
    """UM0050 pp. 70-71: the invalid instruction begins at stacked PC - 1."""

    initial = {
        "pc": 0x1234,
        "sp": 0x8000,
        "r": 0,
        "ram": [[0x1234, 0xED], [0x1235, 0x31]],
        "z180": {"itc": 0x01},
    }

    final = trap_transition(initial)
    memory = ram_dict(final)

    assert memory[0x7FFF] == 0x12
    assert memory[0x7FFE] == 0x35
    assert final["r"] == 2
    assert final["z180"]["itc"] == 0x81


def test_third_opcode_trap_stacks_the_undefined_fetch_address() -> None:
    """UM0050 pp. 70-72: the invalid instruction begins at stacked PC - 2."""

    initial = {
        "pc": 0x1234,
        "sp": 0x8000,
        "r": 0,
        "ram": [
            [0x1234, 0xDD],
            [0x1235, 0xCB],
            [0x1236, 0x05],
            [0x1237, 0x40],
        ],
        "z180": {"itc": 0x01},
    }

    final = trap_transition(initial)
    memory = ram_dict(final)

    assert memory[0x7FFF] == 0x12
    assert memory[0x7FFE] == 0x36
    assert final["r"] == 3
    assert final["z180"]["itc"] == 0xC1


def test_checked_in_corpus_matches_appendix_c() -> None:
    validate_corpus(CHECKED_IN_CORPUS)


def test_complete_generation_is_byte_identical(tmp_path: Path) -> None:
    first = tmp_path / "first"
    second = tmp_path / "second"
    generate_corpus(first)
    generate_corpus(second)
    validate_corpus(first)
    validate_corpus(second)
    compare_corpora(first, second)
