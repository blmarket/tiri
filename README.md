# tiri

c2rust translation of tmux

Pane status comes directly from OSC 7501 reports. `#{pane_agent}` exposes the
reported application, `#{pane_agent_state}` exposes `idle`, `working`, `done`,
`blocked`, or `error` (`none` without a report), and `#{pane_state_emoji}` renders
the reported state. Applications that do not report status have no inferred
agent state. There is no periodic agent process scan, screen classification,
or session/model discovery. The former `pane_agent_pid`,
`pane_agent_session_id`, and `pane_agent_model` variables have been removed.

The Git plugin refreshes repository metadata independently every 500 ms.
`TMUX_C2RS_PLUGINS` selects plugins (`git` by default, `none` to disable them);
OSC 7501 status is built in and does not depend on plugins.
