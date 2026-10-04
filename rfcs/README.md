# N3 proposals

Concrete ideas whose rationale, tradeoffs, or open questions are worth preserving.
RFC means **Request for Comments**: these documents invite discussion and do not
promise a feature or schedule.

## Where an idea belongs

- Keep short, self-explanatory work items in [TODO.md](../TODO.md).
- Give an idea an RFC when a keyword would lose meaningful context: intended UX,
  alternatives, constraints, risks, or unresolved decisions. Link it from TODO.
- Use [docs/research](../docs/research/) for exploratory research and historical
  notes, and [docs/architecture](../docs/architecture/) for implemented contracts.
- Keep the [user guide](../docs/guide/README.md) about implemented behavior.
  RFCs are repository-owned documentation; user guides must not link to them.

## Keeping proposals lightweight

Use the next available `NNNN-short-name.md` filename. Include a status, date,
problem, proposed direction, rationale, and open questions; add examples,
alternatives, references, or validation criteria when they help. Scale the
document to the idea rather than filling a mandatory template.

Use **Draft** while exploring, **Deferred** when deliberately setting the idea
aside, **Accepted** after an explicit decision on scope, **Implemented** when the
agreed behavior lands, or **Rejected** when a recorded decision is useful.
Record the reason when the status changes. A stored proposal is not an
instruction to implement it; acceptance and implementation remain separate steps.

When implementation lands, update its RFC status and TODO entry, then document
the actual behavior in architecture and executable user guides as appropriate.
Keep unresolved ideas out of those implemented contracts. Discarded fragments
that contain no useful reasoning do not need an RFC.

## Index

| RFC                                                | Status              | Topic                                                                                      |
| -------------------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------ |
| [0001](0001-paired-selection-in-2d.md)             | Deferred            | Pair corresponding front/back vertices during selection in an aligned 2D view.             |
| [0002](0002-authored-scenes.md)                    | Draft               | N3-owned scenes, editor/history boundaries, migration, and interchange feasibility.        |
| [0003](0003-executable-documentation-framework.md) | Implemented locally | A reusable Rust-first documentation SDK, evidence contract, and staged extraction from N3. |
