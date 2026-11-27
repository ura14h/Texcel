# Texcel local patches

This directory holds the `ironcalc_base` 0.9.1 source from crates.io. The
original `LICENSE-MIT`, `LICENSE-Apache-2.0`, README, and
`THIRD-PARTY-NOTICES.md` are kept unchanged. Texcel selects this copy through
`[patch.crates-io]` in the root `Cargo.toml`. The changes below are offered
under the same license as this crate (MIT OR Apache-2.0), not under Texcel's MIT
license. The range change below is marked with a `Texcel patch` comment in the
source.

## Exponent sign in the number formatter

The formatter now takes the exponent sign from the exponent it calculated.
Values in `[1, 10)` have exponent zero and are no longer written with a
negative sign. The vendored formatter test covers this case.

## References to a deleted sheet

`Model::delete_sheet` replaces cell and range references to the deleted sheet
with `#REF!` before the sheet is removed. Formulas without such a reference keep
their stored text. A later sheet with the same name therefore cannot revive a
deleted reference.

## Ranges that lose some rows or columns

`src/expressions/parser/stringify.rs`: when rows or columns are deleted, the
release moved each end of a range separately, so a range whose first or last
row was deleted became `#REF!`. Like Excel, a range now shrinks to the rows and
columns that remain (for example, deleting row 1 turns `SUM(A1:A3)` into
`SUM(A1:A2)`). It becomes `#REF!` only when every row or every column of the
range is deleted.

## Function name list

`src/lib.rs`: `function_names` returns the localized names of every function the
engine implements. The `functions` module is private in the release, so Texcel
cannot otherwise list the built-in functions for its function picker. The
function is marked with a `Texcel patch` comment.

## References that follow a cut range

`src/model.rs`: `Model::displace_references_to_area` rewrites a formula that
stays in its cell so that its references into a cut range point to where the
range was pasted. It reuses the release's cut-and-paste rule for moved formulas
(`move_formula`): a reference or a whole range inside the area moves, and other
references keep their cells. The release only rewrites formulas inside the moved
area, so Texcel could not otherwise make other cells follow a cut and paste like
Excel. The function is marked with a `Texcel patch` comment.
