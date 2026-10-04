# SpaceTerm fork

SpaceTerm uses this AccessKit fork to preserve Terminal caret geometry, complete word boundaries,
bounded AT-SPI text events, and efficient sparse text updates. Cargo pins the core, consumer, and
AT-SPI translation crates to an immutable `spaceterm-YYYY-MM-DD` tag at
`https://github.com/sadiksaifi/accesskit`. Another release on the same date adds `.1`, `.2`, and so on.
Never move or delete a published tag.

## Upstream base

The initial upstream base is `c88605b96d04431f9c3c792464a0f2f253480e94`
(`chore: release main (#735)`, 2026-07-14): `accesskit` 0.24.1 in `common/`,
`accesskit_consumer` 0.38.0 in `consumer/`, and `accesskit_atspi_common` 0.19.1 in
`platforms/atspi-common/`.

The patches were ported from SpaceTerm's `feat/linux-support` commit
`131851ca9792d33c4fcf7fd901ea3ed2ab4288af`. Source trees were compared with the retained copies
before their removal. Upstream workspace manifests are retained, with a `serde_json` test
dependency for the core's serialization regressions.

## Patches

- `1f47948` `feat(text): support terminal caret geometry and wide word boundaries`
- `66b7c2d` `fix(atspi): preserve terminal caret geometry and bound text events`
- `4a27c32` `perf(text): retain incremental indexes for sparse terminal updates`
- `4adf345` `test(text): import the vector macro for default feature builds`

The last patch adds an `alloc::vec` import required by the shared-children test when the core
builds without its optional standard-library features. It was also applied to the retained copy
before the final source comparison. Every retained patch test is included.

## Updating

1. Fetch upstream main from `https://github.com/AccessKit/accesskit` and rebase `spaceterm` onto it.
   Review the patches against upstream changes and update the upstream base above.
2. Run the validation below, then commit source and documentation changes with Conventional Commits.
3. Create an annotated tag with `git tag -a <tag> -m 'SpaceTerm AccessKit <tag>'`, using the date
   scheme above. Push the fork branch and new tag to `sadiksaifi/accesskit`.
4. In SpaceTerm, disable any local AccessKit override and update all three `[patch.crates-io]`
   entries to the published tag. Refresh the lockfile with
   `mise exec -- cargo update -p accesskit -p accesskit_consumer -p accesskit_atspi_common`.
5. Run `mise run check`, `mise run lint:rust`, `mise run fmt`, `mise run test:one accesskit`, and
   `mise run test:one accessibility`. Commit the manifest and lockfile together.

## Local development

In SpaceTerm, `mise run accesskit:local:on /path/to/accesskit` adds an owned `[patch.crates-io]`
block to `.cargo/config.toml`, overriding the pinned Git sources for all three crates without
fetching the fork. `mise run accesskit:local:off` removes only that block and resolves the published
tag again. Keep the local configuration addition and any local lockfile changes uncommitted.
SpaceTerm's existing environment and local GPUI configuration are preserved.

## Validation

Use SpaceTerm's Rust toolchain and an external target directory:

```sh
export CARGO_TARGET_DIR=/home/sdk/.worktrees/accesskit-target
mise exec rust@1.98.1 -- cargo test -p accesskit -p accesskit_consumer -p accesskit_atspi_common
mise exec rust@1.98.1 -- cargo test -p accesskit -p accesskit_consumer -p accesskit_atspi_common --features accesskit/serde,accesskit/schemars
mise exec rust@1.98.1 -- cargo clippy -p accesskit -p accesskit_consumer -p accesskit_atspi_common --all-targets -- -D warnings
mise exec rust@1.98.1 -- cargo clippy -p accesskit -p accesskit_consumer -p accesskit_atspi_common --all-targets --features accesskit/serde,accesskit/schemars,accesskit/enumn,accesskit_atspi_common/simplified-api -- -D warnings
mise exec rust@1.98.1 -- cargo fmt --all -- --check
```

All-feature test and Clippy checks additionally need a Python development library for the
optional PyO3 bindings. Set `PYO3_PYTHON` to an interpreter with its matching shared library;
set the runtime library search path if that interpreter is installed outside the system prefix.
Generated files and build outputs must remain uncommitted.
