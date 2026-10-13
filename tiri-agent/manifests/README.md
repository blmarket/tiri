# Agent detection rules

tiri reads an agent pane's state (`idle`, `working`, `blocked`) from its screen
and terminal title, using rules kept here as data. The engine that evaluates
them is `src/integration/manifest.rs`.

## Layout

- `herdr/` holds herdr's detection manifests, copied unmodified from
  [herdr](https://github.com/herdrdev/herdr) `src/detect/manifests/`
  (Apache-2.0, see `herdr/LICENSE`). Last synced from herdr commit
  `3d9d2b18dab139ba226ebc5a1c9a9f2c9c3ee4df` (2026-10-06).
- `tiri/` holds one overlay per agent: the rules tiri adds, replaces, or
  disables on top of herdr's. Each file explains why it differs.

| Agent | herdr manifest | tiri overlay |
| --- | --- | --- |
| Claude Code | `herdr/claude.toml` | `tiri/claude.toml` |
| Codex | `herdr/codex.toml` | `tiri/codex.toml` |
| Pi | `herdr/pi.toml` | `tiri/pi.toml` |
| Antigravity CLI | `herdr/antigravity.toml` | `tiri/agy.toml` |
| OpenCode | `herdr/opencode.toml` | `tiri/opencode.toml` |

## Rules

A rule names a `state`, a `priority`, the `region` of the screen it reads, and
matchers that must all hold:

- `contains`: every needle occurs in the region, ignoring case;
- `regex`: every pattern matches the region;
- `line_regex`: each pattern matches some line of the region;
- `all` / `any` / `not`: nested gates, all / at least one / none of which
  match.

The highest-priority matching rule decides; equal priorities go to the rule
listed first. `skip_state_update = true` (with `state = "unknown"`) keeps the
previous state, for transient UI such as a transcript viewer. When no rule
matches a pane whose process is a known agent, the agent is idle.

Regions are herdr's: `whole_recent` (the visible screen, the default),
`bottom_lines(n)`, `bottom_non_empty_lines(n)`, `top_non_empty_lines(n)`,
`after_last_horizontal_rule`, `prompt_box_body`, `above_prompt_box`,
`last_non_empty_above_prompt_box`, the Codex prompt regions
(`after_last_prompt_marker`, `before_current_prompt_marker`,
`whole_recent_without_current_prompt_marker`, `current_prompt_block_marker`,
`after_current_prompt_block_marker`), and `osc_title`. `osc_progress` is
accepted but never matches: tiri does not capture OSC 9;4.

## Overlays

An overlay is a manifest with the agent's `id` whose rules replace the rule of
the same id or are added, and whose `disable` list removes rules by id. Naming
a rule that does not exist is an error, so a rule renamed upstream cannot
orphan its override.

A bundled overlay declares the herdr manifest `version` it was reviewed
against, and loading fails when that version differs.

### Your own overlay

tiri layers `$XDG_CONFIG_HOME/tiri/agent-detection/<agent>.toml` (default
`~/.config/tiri/agent-detection/`) over the bundled rules when the server
starts, where `<agent>` is `claude`, `codex`, `pi`, `agy`, or `opencode`. An
overlay that fails to load is logged and ignored. A user overlay may omit
`version`. For example, to read a custom footer as blocked:

```toml
id = "claude"

[[rules]]
id = "my_review_footer"
state = "blocked"
priority = 990
region = "bottom_non_empty_lines(3)"
contains = ["review requested"]
```

## Updating from herdr

1. Copy the five manifests from herdr's `src/detect/manifests/` into `herdr/`
   (`antigravity.toml` keeps its name) and record the commit above.
2. Run `cargo test -p tiri-agent`. `bundled_rules_load` fails for every
   overlay whose herdr manifest changed version, and for every overlay naming
   a rule that no longer exists.
3. For each failure, diff the herdr manifest, revisit the overlay's
   replacements and additions against it, and update the overlay's `version`.
4. A manifest that raises `min_engine_version` above the engine's
   `ENGINE_VERSION` needs the engine ported first.
