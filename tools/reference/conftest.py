"""Hypothesis profiles and durable example database for Phase 8."""

from pathlib import Path

from hypothesis import HealthCheck, settings
from hypothesis.database import DirectoryBasedExampleDatabase


DATABASE = DirectoryBasedExampleDatabase(Path(__file__).parent / ".hypothesis")
COMMON = {
    "database": DATABASE,
    "deadline": None,
    "suppress_health_check": (HealthCheck.too_slow,),
}

settings.register_profile("gate", max_examples=2_000, **COMMON)
settings.register_profile("nightly", max_examples=50_000, **COMMON)
