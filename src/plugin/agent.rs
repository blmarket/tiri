//! Existing tiri-agent detectors hosted through tiri pane observations.
use super::{Host, PaneActivity, PaneId, PaneValues, Plugin, Variable};
use std::ffi::CString;
use std::io;
use std::time::Duration;
use tiri_agent::integration::status::{AgentStatus, StatusHub};
use tiri_agent::integration::AgentObserver;
use tiri_agent::pane_class::{PaneClass, PaneProcessProbe};

const VARIABLES: &[Variable] = &[
    Variable::new(c"pane_agent"),
    Variable::with_default(c"pane_agent_state", c"none"),
    Variable::new(c"pane_agent_pid"),
    Variable::new(c"pane_agent_session_id"),
    Variable::new(c"pane_agent_model"),
    Variable::with_default(c"pane_state_emoji", c"🛑"),
];

pub struct AgentPlugin {
    observer: AgentObserver,
    hub: StatusHub,
}

impl AgentPlugin {
    pub fn new() -> Self {
        let hub = StatusHub::new();
        Self {
            observer: AgentObserver::new(hub.clone()),
            hub,
        }
    }
}

impl Default for AgentPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AgentPlugin {
    fn name(&self) -> &'static str {
        "agent"
    }
    fn variables(&self) -> &'static [Variable] {
        VARIABLES
    }
    fn interval(&self) -> Duration {
        AgentObserver::INTERVAL
    }

    fn refresh(&mut self, host: &dyn Host, output: &mut PaneValues) -> io::Result<()> {
        self.observer.tick(host);
        let snapshot = self.hub.snapshot();
        for pane in host.pane_ids()? {
            publish(
                output,
                pane,
                snapshot.panes.get(&pane),
                host.pane_activity(pane),
            )?;
        }
        Ok(())
    }
}

fn publish(
    output: &mut PaneValues,
    pane: PaneId,
    status: Option<&AgentStatus>,
    activity: Option<PaneActivity>,
) -> io::Result<()> {
    output.set(
        pane,
        c"pane_agent",
        CString::new(status.map_or("", |s| s.agent)).expect("agent name"),
    )?;
    output.set(
        pane,
        c"pane_agent_state",
        CString::new(status.map_or("none", |s| s.state.wire_str())).expect("agent state"),
    )?;
    output.set(
        pane,
        c"pane_agent_pid",
        status
            .and_then(|s| s.pid)
            .map(|pid| CString::new(pid.to_string()).expect("pid"))
            .unwrap_or_default(),
    )?;
    output.set(
        pane,
        c"pane_agent_session_id",
        status
            .and_then(|s| s.session_id.clone())
            .unwrap_or_default(),
    )?;
    output.set(
        pane,
        c"pane_agent_model",
        status.and_then(|s| s.model.clone()).unwrap_or_default(),
    )?;
    let emoji = if let Some(status) =
        status.filter(|s| !s.agent.is_empty() && !s.state.emoji().is_empty())
    {
        status.state.emoji()
    } else if let Some(info) = activity {
        let probe = PaneProcessProbe::new(info.foreground, info.session_leader, info.command);
        PaneClass::classify(Some(&probe), info.alternate_on, info.exited).emoji()
    } else {
        PaneClass::Dead.emoji()
    };
    output.set(
        pane,
        c"pane_state_emoji",
        CString::new(emoji).expect("emoji"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiri_agent::integration::AgentState;

    #[test]
    fn publishes_agent_metadata_and_classifies_non_agent_panes() {
        let mut values = PaneValues::new(VARIABLES);
        let status = AgentStatus {
            agent: "codex",
            pid: Some(123),
            session_id: Some(c"session".into()),
            model: Some(c"gpt-luna".into()),
            state: AgentState::Working,
        };
        publish(&mut values, PaneId(1), Some(&status), None).unwrap();
        let pane = &values.panes[&1];
        assert_eq!(pane[c"pane_agent"], c"codex");
        assert_eq!(pane[c"pane_agent_pid"], c"123");
        assert_eq!(pane[c"pane_agent_model"], c"gpt-luna");
        assert_eq!(pane[c"pane_agent_session_id"], c"session");
        assert_eq!(pane[c"pane_state_emoji"], c"🔄");
        assert_eq!(pane.len(), VARIABLES.len());
        publish(
            &mut values,
            PaneId(2),
            None,
            Some(PaneActivity {
                foreground: None,
                session_leader: None,
                command: None,
                alternate_on: true,
                exited: false,
            }),
        )
        .unwrap();
        assert_eq!(values.panes[&2][c"pane_state_emoji"], c"🪟");
        assert_eq!(values.panes[&2][c"pane_agent_state"], c"none");
        publish(&mut values, PaneId(3), None, None).unwrap();
        assert_eq!(values.panes[&3][c"pane_state_emoji"], c"🛑");
    }
}
