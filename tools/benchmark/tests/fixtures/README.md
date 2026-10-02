# Measurement contract fixtures

These are authored test data, not captured performance results.

- `measurement-schema.json` lists the public v2 sample, stage, counter, and
  validity field names. Rust checks actual serialization against it; Python
  checks its independent admission schema and test records against it.
- `editor-capture.json` is a small synthetic editor record with three CPU frame
  times: 10, 20, and 90 ms. Its deliberately incorrect precomputed summary must
  be ignored. The CLI test changes explicit raw values to establish known
  medians, tails, and before/after deltas without using the production statistics
  functions to derive expected answers. Metadata and camera matrices are
  placeholders for schema/aggregation tests, not a claim about real hardware.

Review changes to these fixtures with their assertions. Do not automatically
update them after a failing test. Use real host captures separately to verify
collection, platform behavior, and renderer execution.
