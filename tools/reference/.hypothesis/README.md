# Hypothesis example database

P8.6 configures both the `gate` and `nightly` profiles to use this checked-in
directory. Hypothesis adds hashed regression entries here when it discovers a
failing example. Those entries are retained in Git in addition to the required
explicit `@example(...)` and `tests/z180-sst/` regression.
