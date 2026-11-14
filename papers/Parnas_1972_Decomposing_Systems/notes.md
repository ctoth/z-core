---
title: "On the Criteria To Be Used in Decomposing Systems into Modules"
authors: "D. L. Parnas"
year: 1972
venue: "Communications of the ACM 15(12)"
doi_url: "https://doi.org/10.1145/361598.361623"
pages: "1053-1058"
---

# Reading notes

Provenance: all six rendered page images read directly on 2026-11-14 using
paper-reader; no extracted paper text used. These are concise notes rather
than a replacement for the copyrighted paper.

## Argument and evidence

Modules assign responsibilities, not necessarily subprograms (p.1054).
The KWIC example contrasts processing stages with interfaces hiding storage
and ordering choices (pp.1054-1055). Changeability, independent development,
and comprehension motivate the comparison (pp.1055-1056).

The phrase "information hiding" identifies the central criterion (p.1056).
Even the improved example unnecessarily specifies shift ordering (p.1056).
Procedure-call overhead can undermine efficiency; module boundaries need not
remain runtime call boundaries (p.1057). Compiler/interpreter reuse illustrates
the approach (p.1057). Hierarchy and encapsulation are independent (p.1058).

## Limitations

Conceptual examples, not measured ABI performance or a compatibility standard.
No empirical parameter estimates or statistical equations (pp.1053-1058).

## Application to this project

Our design inference: hide machine storage behind handles, retain direct core
execution, and specify lifetime/error semantics explicitly. Actual C layout,
allocator, callback, and unwind rules require Rust documentation and tests.

## Collection Cross-References

First collected paper; no forward matches, reverse citations, or conceptual
links to other collected papers. Follow-up leads are listed in citations.md.
