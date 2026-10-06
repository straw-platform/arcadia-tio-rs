# Arcadia TIO Rust wrappers agent routing

[Arcadia Ops AGENTS.md](../arcadia-ops/AGENTS.md) is the sole operating-policy
entry point for governed Arcadia agent work. Read it first unless already loaded
for this context/version, then read this repository's
[business contract](BUSINESS_CONTRACT.md) and only the task-relevant leaf documents.

Do not duplicate coordination, authorization, outputs or campaign state here.
The business contract owns component correctness; Ops owns shared execution rules.
If they appear inconsistent, stop the affected decision and report the conflict
rather than weakening either contract. Existing holds and frozen assignments survive
documentation changes. Reading an example command is not authority to execute it.

This routing is for agent work; it adds no private-source or Ops dependency to the
public library, exported authoring interface or delivered runtime.
