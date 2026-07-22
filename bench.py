"""Compare the three Python-facing execution modes required by PLAN P8.4."""

from __future__ import annotations

import statistics
import time
from collections.abc import Callable
from dataclasses import dataclass

from qns.cpu import CFFI_AVAILABLE, Z180 as CffiZ180
from z180 import Machine
from z180.compat import Z180 as CompatZ180


MIN_SAMPLE_SECONDS = 0.25
SAMPLE_COUNT = 5
INITIAL_BUDGET = 100_000
MAX_BUDGET = 1_600_000_000


def nop_read(_address: int) -> int:
    return 0x00


def discard_write(_address: int, _value: int) -> None:
    return None


def make_compat() -> CompatZ180:
    return CompatZ180(mem_read=nop_read, mem_write=discard_write)


def make_internal() -> Machine:
    return Machine(
        config_dict={
            "regions": [{"base": 0, "size": 0x10000, "kind": "ram"}],
        }
    )


def make_cffi() -> CffiZ180:
    return CffiZ180(mem_read=nop_read, mem_write=discard_write)


@dataclass(frozen=True)
class Result:
    mode: str
    budget: int
    cycles_per_second: float
    sample_seconds: float


def timed_run(machine: object, budget: int) -> tuple[int, float]:
    start = time.perf_counter()
    actual = machine.run(budget)
    elapsed = time.perf_counter() - start
    if actual <= 0:
        raise RuntimeError(f"run({budget}) returned non-positive cycle count {actual}")
    return actual, elapsed


def choose_budget(machine: object) -> int:
    budget = INITIAL_BUDGET
    while True:
        _actual, elapsed = timed_run(machine, budget)
        if elapsed >= MIN_SAMPLE_SECONDS or budget >= MAX_BUDGET:
            return budget
        budget = min(budget * 2, MAX_BUDGET)


def benchmark(mode: str, factory: Callable[[], object]) -> Result:
    machine = factory()
    budget = choose_budget(machine)
    rates = []
    elapsed_samples = []
    for _ in range(SAMPLE_COUNT):
        actual, elapsed = timed_run(machine, budget)
        rates.append(actual / elapsed)
        elapsed_samples.append(elapsed)
    return Result(
        mode=mode,
        budget=budget,
        cycles_per_second=statistics.median(rates),
        sample_seconds=statistics.median(elapsed_samples),
    )


def main() -> None:
    if not CFFI_AVAILABLE:
        raise RuntimeError("qns old CFFI binding is unavailable")

    results = [
        benchmark("compat callback", make_compat),
        benchmark("internal memory", make_internal),
        benchmark("old CFFI", make_cffi),
    ]

    print("mode                 budget       median seconds       cycles/sec")
    print("-------------------  -----------  -----------------  ---------------")
    for result in results:
        print(
            f"{result.mode:<19}  {result.budget:>11,}  "
            f"{result.sample_seconds:>17.6f}  {result.cycles_per_second:>15,.0f}"
        )


if __name__ == "__main__":
    main()
