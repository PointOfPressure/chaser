# Flag ground truth

Two dumps of the Roblox engine's own flag symbols. `core/src/validate.rs`
embeds both with `include_str!` and uses them to decide whether a FastFlag is
real before Chaser writes it into Sober's config.

Neither file is hand-edited. Both go stale as Roblox ships new builds; stale
data makes Chaser call a *new* flag fabricated, so refresh them when that
starts happening.

## `flags.txt` — 16281 lines, `PREFIX:Name`

Every flag symbol in an unstripped Roblox Studio build.

- Source build: Roblox Studio macOS `0.735.19.7351131`
- Prefixes seen: `DFInt`, `FFlag`, `DFFlag`, `FInt`, `FLog`, `FString`,
  `DFLog`, `DFString`, `SFFlag`, `SFString`

This is the **only** source that records a flag's prefix, which is what lets
Chaser catch `FFlag` / `DFFlag` mix-ups and suggest the right one.

To regenerate from a binary that still carries its symbols:

```sh
grep -aoE '\b[DS]?F(Flag|Int|String|Log|PFlag)[A-Z][A-Za-z0-9_]{2,}' RobloxStudio \
  | sort -u > /tmp/raw.txt
# then split each name into PREFIX:Name, one per line, sorted unique
```

Unstripped Roblox builds are not something you can rely on being available.
If you cannot get one, keep the existing file: an old prefix map is still
correct for every flag that already existed.

## `player-flags.txt` — 14265 lines, bare names

Bare flag names from a Player-client offset dump. No prefixes, so this can only
answer "does this symbol exist", never "under which prefix".

- Roblox version: `version-f5a60436d48947d3`, dumped 2026-08-27
- Upstream: `https://offsets.imtheo.lol/FFlags.hpp`, mirrored at
  <https://github.com/4anti/Roblox-Offsets-ALL> (refreshed every six hours)

It exists because the Studio dump above is frozen: roughly 1500 flags in the
current Player client are newer than that build, and without this file Chaser
would report every one of them as fabricated.

To regenerate:

```sh
curl -s https://offsets.imtheo.lol/FFlags.hpp \
  | grep -oE 'uintptr_t\s+[A-Za-z0-9_]+' \
  | awk '{print $2}' \
  | grep -vE '^(Pointer|ToFlag|ToValue)$' \
  | sort -u > core/data/player-flags.txt
```

## What is deliberately *not* here

Roblox's allowlist. It is 18 entries and lives in `ALLOWLIST` in
`core/src/validate.rs`, because it is short, it changes on Roblox's schedule
rather than per-build, and the tests need to read it directly.
