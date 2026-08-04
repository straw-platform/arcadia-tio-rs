#!/usr/bin/env python3
'''Enforce exact post-split line-count ratchets for public implementation modules.'''

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parent.parent


@dataclass(frozen=True)
class Budget:
    relpath: str
    max_lines: int


BUDGETS: tuple[Budget, ...] = (
    # Canonical C-ABI-free OCB core facade and Phase 6 child ownership.
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle.rs", 5261),
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle/attribution.rs", 85),
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle/execution.rs", 969),
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle/materialize.rs", 739),
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle/planning.rs", 523),
    Budget("crates/arcadia-tio-ocb-core/src/column_bundle/tests.rs", 3697),
    # Safe wrapper facade and focused implementation families.
    Budget("crates/arcadia-tio-rs/src/conversion.rs", 942),
    Budget("crates/arcadia-tio-rs/src/coordinates.rs", 3616),
    Budget("crates/arcadia-tio-rs/src/error.rs", 203),
    Budget("crates/arcadia-tio-rs/src/file_types.rs", 2478),
    Budget("crates/arcadia-tio-rs/src/lib.rs", 76),
    Budget("crates/arcadia-tio-rs/src/ocb.rs", 32),
    Budget("crates/arcadia-tio-rs/src/ocb/conversion.rs", 2759),
    Budget("crates/arcadia-tio-rs/src/ocb/maintenance.rs", 203),
    Budget("crates/arcadia-tio-rs/src/ocb/model.rs", 1888),
    Budget("crates/arcadia-tio-rs/src/ocb/read.rs", 371),
    Budget("crates/arcadia-tio-rs/src/ocb/session.rs", 91),
    Budget("crates/arcadia-tio-rs/src/ocb/write.rs", 47),
    Budget("crates/arcadia-tio-rs/src/ops.rs", 2104),
    Budget("crates/arcadia-tio-rs/src/ownership.rs", 183),
    Budget("crates/arcadia-tio-rs/src/tensor.rs", 1842),
    Budget("crates/arcadia-tio-rs/src/tensor_file.rs", 5225),
    Budget("crates/arcadia-tio-rs/src/tests.rs", 1361),
    Budget("crates/arcadia-tio-rs/src/typed_ops.rs", 344),
    # Raw sys facade and ABI-family modules.
    Budget("crates/arcadia-tio-sys/src/common.rs", 12),
    Budget("crates/arcadia-tio-sys/src/history.rs", 26),
    Budget("crates/arcadia-tio-sys/src/lib.rs", 35),
    Budget("crates/arcadia-tio-sys/src/lifecycle_coordinates.rs", 527),
    Budget("crates/arcadia-tio-sys/src/maintenance.rs", 140),
    Budget("crates/arcadia-tio-sys/src/metadata.rs", 71),
    Budget("crates/arcadia-tio-sys/src/mutation.rs", 408),
    Budget("crates/arcadia-tio-sys/src/ocb.rs", 531),
    Budget("crates/arcadia-tio-sys/src/read.rs", 454),
    Budget("crates/arcadia-tio-sys/src/tensor.rs", 152),
    Budget("crates/arcadia-tio-sys/src/types.rs", 4144),
)


def line_count(path: Path) -> int:
    with path.open("r", encoding="utf-8") as handle:
        return sum(1 for _ in handle)


def main() -> int:
    failures: list[str] = []
    for budget in BUDGETS:
        path = ROOT / budget.relpath
        if not path.is_file():
            failures.append(f"{budget.relpath}: file is missing")
            continue
        actual = line_count(path)
        status = "ok" if actual <= budget.max_lines else "over"
        print(f"[module-size] {status:4} {budget.relpath} ({actual}/{budget.max_lines})")
        if actual > budget.max_lines:
            failures.append(
                f"{budget.relpath}: {actual} lines exceeds budget {budget.max_lines}"
            )
    if failures:
        print("\n[module-size] budget violations detected:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        print(
            "[module-size] split by the existing ownership families; do not raise the ratchet.",
            file=sys.stderr,
        )
        return 1
    print("[module-size] all budgets satisfied")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
