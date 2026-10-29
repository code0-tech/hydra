# codezero

`codezero` — a CLI setup wizard for running [CodeZero](https://codezero.build) locally.

```sh
npm install -g codezero
codezero
```

This package is a thin Node.js wrapper: on install it downloads the prebuilt `codezero` binary
for your OS/arch from the matching [GitHub release](https://github.com/code0-tech/hydra/releases).
If the postinstall download is skipped (e.g. `pnpm` without `trustedDependencies`, Deno, or
`--ignore-scripts`), the wrapper downloads the binary lazily on first run instead.

Supported platforms: Linux x64, macOS x64/arm64, Windows x64.

Source, issues, and other install methods (Homebrew, Cargo, prebuilt archives): see the
[hydra repository](https://github.com/code0-tech/hydra).
