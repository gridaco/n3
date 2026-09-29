# Contributing to N3

Read [AGENTS.md](AGENTS.md) for shared development principles, code ownership,
documentation requirements, and the iteration workflow. Those conventions apply
to human and automated contributions alike.

Install Rust, `just`, and Python 3. Native macOS development also needs the
command-line developer tools. Docker is optional, for reproducing Ubuntu CI. Install Node.js 24/npm
with `npx`, then run `just setup` to warm the pinned Oxfmt cache and activate the
repository's pre-push hook. The optional CI container supplies Node. Run
`just fmt` before checking your changes with `just verify`.

- [Development](docs/development.md): setup, commands, and document/import formats.
- [Architecture](docs/architecture/architecture.md): responsibilities and state boundaries.
- [Proposals](rfcs/README.md): concrete future ideas, rationale, and unresolved decisions; short work items stay in [TODO.md](TODO.md).
- [Documentation pipeline](docs/architecture/documentation-pipeline.md): executable guides, input replay, callouts, and animation authoring.

Keep changes focused and describe the behavior they add or fix. For implementation
changes, run `just verify` and report any device interactions that still
need manual validation. User-visible changes should update their owning guide
scenario and template, with generated media reviewed alongside the code.

The pre-push hook requires a clean checkout and pushed commits matching `HEAD`,
then runs `just verify`. It never stages, formats, commits, or pushes for you.
`just verify` and the hook run locally: formatting, lint, tooling tests, and the
full Rust suite with exact guide replay. `just ci`, `just ci-test`, and
`just ci-docs check/update` explicitly opt into the pinned Ubuntu `linux/amd64`
Docker environment. These CI commands compare against a baseline made with that
renderer; Metal and lavapipe media are not assumed identical. The separate macOS
lane checks the app bundle and native integration. Use `just test-metal` for
focused Metal capture checks. See
[verification and hooks](docs/development.md#verification-and-git-hooks).

Use `just docs` to browse the guide locally. It serves the existing generated
pages; use `just docs update` to regenerate them natively and
`just docs check` to verify them without writing. See
[the documentation workflow](docs/development.md#documentation-is-executable)
for preview options and authoring details.
