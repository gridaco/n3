# UX ideas

Working notes, begun September 28, 2026. These capture product ideas separately
from the engineering foundation. They are not a feature checklist or approved
interaction specification.

## Intent

3D modeling often feels painful and unintuitive. Blender is the modeling
reference for this project; the research should focus on understanding its
operations and where interactions demand too much prior knowledge.

The desired feel is the approachability of Figma combined with a pen tool for
drawing topology. Users should be able to express a shape directly, with the
software making useful decisions and keeping the result clean by default.

V1 is deliberately minimal and focuses on hard surfaces. Sculpting is excluded.
macOS is the initial context for interaction research.

## Initial topics from the brief

| Topic                | Idea or question to retain                                                                                                                |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Object creation      | Adding an object should be an obvious, small action. Which objects are essential is open.                                                 |
| Component selection  | Support thinking about vertices, edges, and faces. How people select or switch between them is open.                                      |
| Rotation             | Define camera orbit and object/component rotation separately, including pivots and axis behavior. No gestures or keys have been assigned. |
| Navigation           | Explore mouse and Mac trackpad behavior alongside selection and editing.                                                                  |
| Keyboard             | Design a small, understandable set of keys and modifiers; Blender bindings are references to examine.                                     |
| Drawing topology     | Explore a pen-like flow for placing points, connecting edges, and forming surfaces. Working planes and depth intent remain open.          |
| Helpful defaults     | Reduce the need to know operation names and predict heuristic outcomes. Keep inferred choices understandable and recoverable.             |
| Operations and nodes | Document useful operations and possible compositions. A procedural node graph or node editor has not been selected.                       |

## What does clean topology mean?

This needs a definition before becoming a promise. Several different goals are
currently bundled into the phrase:

- Structurally valid connections and consistent face boundaries.
- Predictable geometry without accidental duplicates or degenerate faces.
- Useful edge flow and shading for subsequent hard-surface edits.
- Sensible interpretation of an incomplete drawing.

These are not equivalent to requiring every face to be a quad or every object to
be a closed solid. A partially drawn object may intentionally contain loose
edges or open boundaries. Geometry validity and a person's intended shape need
separate evaluation.

## Jev and assistance near the interaction

Reference: [TypeSafe's introduction to System One models and Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev).

TypeSafe presents Jev as a model for typed decisions and probability outputs.
The relevant idea is assistance at specific decision points in a workflow.
Its claims about schema correctness do not establish correct modeling intent
or geometrically valid results. The article's latency figures are vendor claims,
not measurements in this project.

The [API documentation](https://docs.typesafe.ai/introduction) describes Choice,
Score, and Noul questions. The [quick start](https://docs.typesafe.ai/introduction/quickstart)
uses a hosted endpoint; the reviewed documentation does not establish an
offline/on-device deployment option.

Possible experiments, not commitments:

- Rank valid continuations of a pen stroke: extend an edge, connect to an existing
  vertex, or close a face.
- Suggest a working plane or alignment target from a small set of candidates.
- Rank already-valid operation previews using the local modeling context.

Proposed engineering boundary: deterministic code constructs valid candidates;
an optional heuristic or model ranks them; the editor previews and validates the
chosen operation. Basic editing continues without inference. Responses must be
discarded if the document or interaction has changed since the request. Record
the accepted operation so undo/redo does not query the model again.

Whether this is useful, predictable, fast enough, or better than deterministic
rules has not been tested. No AI integration is selected.
