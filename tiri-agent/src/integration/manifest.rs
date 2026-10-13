//! Data-driven agent state rules.
//!
//! Screen and title rules are herdr's detection manifests, vendored unmodified
//! under `manifests/herdr/` (<https://github.com/herdrdev/herdr>, Apache-2.0),
//! so catching up with herdr's detection is a file copy. Each agent's manifest
//! is layered with tiri's own overlay under `manifests/tiri/`, which holds the
//! rules tiri adds or deliberately reads differently, and then with an optional
//! user overlay read from the config directory.
//!
//! The rule language and every region follow herdr's engine
//! (`src/detect/manifest.rs`): a rule matches when all of its matchers do,
//! against the text of its region, and the highest-priority match wins, ties
//! going to the rule listed first. An overlay is a manifest with the same `id`
//! whose rules replace the base rule of the same id or are added to it, and
//! whose `disable` list removes base rules.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use super::{AgentState, Detection};

#[cfg(test)]
mod tests;

/// The newest manifest engine this port implements. A manifest declaring a
/// higher `min_engine_version` needs a region or matcher this engine lacks, so
/// it is refused rather than half-evaluated.
const ENGINE_VERSION: u32 = 3;
const TOP_NON_EMPTY_LINES_ENGINE_VERSION: u32 = 3;
const MAX_TOP_REGION_LINE_COUNT: usize = u16::MAX as usize;

const MAX_RULES_PER_MANIFEST: usize = 128;
const MAX_GATE_DEPTH: usize = 8;
const MAX_TOTAL_GATES: usize = 512;
const MAX_MATCHERS_PER_GATE: usize = 32;
const MAX_TOTAL_MATCHERS: usize = 1024;
const MAX_MATCHER_CHARS: usize = 512;

/// One agent's bundled rules: herdr's manifest and tiri's overlay on it.
pub(crate) struct Bundle<'a> {
    /// The manifest `id`, which is also the user overlay's file stem.
    pub(crate) agent: &'a str,
    pub(crate) herdr: &'a str,
    pub(crate) tiri: &'a str,
}

/// What a rule set reads: the screen sample and the terminal title. tiri does
/// not capture OSC 9;4 progress, so `osc_progress` rules never match.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Input<'a> {
    pub(crate) screen: &'a str,
    pub(crate) title: &'a str,
}

/// A compiled, ordered rule set for one agent.
#[derive(Debug)]
pub(crate) struct Rules {
    /// Sorted by descending priority, ties kept in manifest order, so the first
    /// rule that matches is the one herdr's engine would choose.
    rules: Vec<CompiledRule>,
}

impl Rules {
    /// The bundled rules alone: herdr's manifest under tiri's overlay.
    pub(crate) fn bundled(bundle: &Bundle) -> Self {
        Self::load(bundle, None)
            .unwrap_or_else(|error| panic!("bundled {} rules are invalid: {error}", bundle.agent))
    }

    /// The bundled rules under the user's overlay, when one exists. A user
    /// overlay that cannot be read or does not apply is ignored with a warning,
    /// leaving the bundled rules in force.
    pub(crate) fn configured(bundle: &Bundle) -> Self {
        let Some(path) = user_overlay_path(bundle.agent) else {
            return Self::bundled(bundle);
        };
        let overlay = match std::fs::read_to_string(&path) {
            Ok(overlay) => overlay,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::bundled(bundle);
            }
            Err(error) => {
                tracing::warn!(
                    target: "tiri::integration",
                    path = %path.display(),
                    %error,
                    "ignoring unreadable agent detection overlay"
                );
                return Self::bundled(bundle);
            }
        };
        match Self::load(bundle, Some(&overlay)) {
            Ok(rules) => rules,
            Err(error) => {
                tracing::warn!(
                    target: "tiri::integration",
                    path = %path.display(),
                    %error,
                    "ignoring invalid agent detection overlay"
                );
                Self::bundled(bundle)
            }
        }
    }

    fn load(bundle: &Bundle, user: Option<&str>) -> Result<Self, String> {
        let mut manifest =
            parse(bundle.herdr).map_err(|error| format!("herdr manifest: {error}"))?;
        if !manifest.disable.is_empty() {
            return Err("herdr manifest: only an overlay may disable rules".to_string());
        }
        if manifest.id != bundle.agent {
            return Err(format!(
                "herdr manifest id {} is not {}",
                manifest.id, bundle.agent
            ));
        }
        layer(
            &mut manifest,
            parse(bundle.tiri).map_err(|error| format!("tiri overlay: {error}"))?,
        )
        .map_err(|error| format!("tiri overlay: {error}"))?;
        if let Some(user) = user {
            layer(&mut manifest, parse(user)?)?;
        }
        validate(&manifest)?;
        Self::compile(&manifest)
    }

    fn compile(manifest: &Manifest) -> Result<Self, String> {
        let mut rules = manifest
            .rules
            .iter()
            .map(|rule| {
                CompiledRule::compile(rule)
                    .map_err(|error| format!("rule {} could not be compiled: {error}", rule.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.priority));
        Ok(Self { rules })
    }

    /// The highest-priority rule matching `input`, if any.
    fn matching(&self, input: Input<'_>) -> Option<&CompiledRule> {
        self.rules
            .iter()
            .find(|rule| rule.gate.matches(region(input, &rule.region)))
    }

    /// Classify `input`: the state of the rule that matches it, previous state
    /// kept for a rule that marks transient UI, and `Unknown` when no rule
    /// matches. The caller decides what an unmatched screen means.
    pub(crate) fn detect(&self, input: Input<'_>) -> Detection {
        let rule = self.matching(input);
        tracing::trace!(
            target: "tiri::integration",
            rule = rule.map(|rule| rule.id.as_str()),
            "agent screen classified"
        );
        match rule {
            Some(rule) if rule.skip_state_update => Detection::KeepPrevious,
            Some(rule) => Detection::State(rule.state),
            None => Detection::State(AgentState::Unknown),
        }
    }

    /// The id of the rule that classifies `input`.
    #[cfg(test)]
    pub(crate) fn matched_rule(&self, input: Input<'_>) -> Option<&str> {
        self.matching(input).map(|rule| rule.id.as_str())
    }
}

/// `$XDG_CONFIG_HOME/tiri/agent-detection/<agent>.toml`, falling back to
/// `~/.config` when `XDG_CONFIG_HOME` is unset or not absolute.
fn user_overlay_path(agent: &str) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))?;
    Some(
        config
            .join("tiri")
            .join("agent-detection")
            .join(format!("{agent}.toml")),
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    /// In an overlay: the version of the manifest it was written against.
    version: Option<String>,
    min_engine_version: Option<u32>,
    #[serde(rename = "updated_at")]
    _updated_at: Option<toml::Value>,
    #[serde(default, rename = "aliases")]
    _aliases: Vec<String>,
    /// Overlays only: ids of rules the overlay removes from its base.
    #[serde(default)]
    disable: Vec<String>,
    #[serde(default)]
    rules: Vec<RuleSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSpec {
    id: String,
    state: Option<StateSpec>,
    #[serde(default)]
    priority: i32,
    #[serde(default = "default_region")]
    region: String,
    #[serde(default)]
    visible_idle: bool,
    #[serde(default)]
    visible_blocker: bool,
    #[serde(default)]
    visible_working: bool,
    #[serde(default)]
    skip_state_update: bool,
    #[serde(default)]
    all: Vec<GateSpec>,
    #[serde(default)]
    any: Vec<GateSpec>,
    #[serde(default, rename = "not")]
    not_gate: Vec<GateSpec>,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateSpec {
    #[serde(default)]
    all: Vec<GateSpec>,
    #[serde(default)]
    any: Vec<GateSpec>,
    #[serde(default, rename = "not")]
    not_gate: Vec<GateSpec>,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StateSpec {
    Idle,
    Working,
    Blocked,
    Unknown,
}

impl From<StateSpec> for AgentState {
    fn from(state: StateSpec) -> Self {
        match state {
            StateSpec::Idle => AgentState::Idle,
            StateSpec::Working => AgentState::Working,
            StateSpec::Blocked => AgentState::Blocked,
            StateSpec::Unknown => AgentState::Unknown,
        }
    }
}

fn default_region() -> String {
    "whole_recent".to_string()
}

fn parse(content: &str) -> Result<Manifest, String> {
    let manifest = toml::from_str::<Manifest>(content).map_err(|error| error.to_string())?;
    if let Some(version) = manifest.min_engine_version
        && version > ENGINE_VERSION
    {
        return Err(format!(
            "manifest requires engine {version}, this engine is {ENGINE_VERSION}"
        ));
    }
    Ok(manifest)
}

/// Apply `overlay` to `base`: drop the rules it disables, replace the rules it
/// redefines, and append the rest. Naming a rule the base does not have is an
/// error, so a rule renamed upstream cannot silently orphan its override; an
/// overlay that declares a `version` applies only to that version of its base,
/// so an upstream update cannot slip under an override written for its
/// predecessor.
fn layer(base: &mut Manifest, overlay: Manifest) -> Result<(), String> {
    if overlay.id != base.id {
        return Err(format!(
            "overlay id {} does not match manifest {}",
            overlay.id, base.id
        ));
    }
    if let Some(version) = &overlay.version
        && base.version.as_ref() != Some(version)
    {
        return Err(format!(
            "overlay was written for {} version {version}, not {}",
            base.id,
            base.version.as_deref().unwrap_or("an unversioned manifest")
        ));
    }
    for id in &overlay.disable {
        let before = base.rules.len();
        base.rules.retain(|rule| rule.id != *id);
        if base.rules.len() == before {
            return Err(format!("overlay disables unknown rule {id}"));
        }
    }
    for rule in overlay.rules {
        match base.rules.iter_mut().find(|base| base.id == rule.id) {
            Some(slot) => *slot = rule,
            None => base.rules.push(rule),
        }
    }
    Ok(())
}

fn validate(manifest: &Manifest) -> Result<(), String> {
    if manifest.rules.is_empty() {
        return Err("manifest must contain at least one rule".to_string());
    }
    if manifest.rules.len() > MAX_RULES_PER_MANIFEST {
        return Err(format!(
            "manifest contains {} rules, max is {MAX_RULES_PER_MANIFEST}",
            manifest.rules.len()
        ));
    }
    let mut complexity = Complexity::default();
    for (index, rule) in manifest.rules.iter().enumerate() {
        if rule.id.trim().is_empty() {
            return Err("manifest rule id must not be empty".to_string());
        }
        if manifest.rules[..index]
            .iter()
            .any(|other| other.id == rule.id)
        {
            return Err(format!("rule id {} is not unique", rule.id));
        }
        if rule.skip_state_update {
            if rule.state != Some(StateSpec::Unknown) {
                return Err(format!(
                    "rule {} uses skip_state_update without state = \"unknown\"",
                    rule.id
                ));
            }
            if rule.visible_idle || rule.visible_blocker || rule.visible_working {
                return Err(format!(
                    "rule {} uses skip_state_update with visible state evidence",
                    rule.id
                ));
            }
        }
        let region = Region::parse(&rule.region).ok_or_else(|| {
            format!(
                "rule {} uses invalid region: {}",
                rule.id,
                rule.region.trim()
            )
        })?;
        if matches!(region, Region::TopNonEmptyLines(_))
            && manifest
                .min_engine_version
                .is_some_and(|version| version < TOP_NON_EMPTY_LINES_ENGINE_VERSION)
        {
            return Err(format!(
                "rule {} uses top_non_empty_lines but min_engine_version is below \
                 {TOP_NON_EMPTY_LINES_ENGINE_VERSION}",
                rule.id
            ));
        }
        validate_gate(&rule.gate(), "rule", 0, &mut complexity)
            .map_err(|error| format!("rule {} has invalid matcher gates: {error}", rule.id))?;
    }
    Ok(())
}

#[derive(Default)]
struct Complexity {
    gates: usize,
    matchers: usize,
}

/// A rule's own matchers, viewed as its top-level gate.
struct GateView<'a> {
    all: &'a [GateSpec],
    any: &'a [GateSpec],
    not_gate: &'a [GateSpec],
    contains: &'a [String],
    regex: &'a [String],
    line_regex: &'a [String],
}

impl RuleSpec {
    fn gate(&self) -> GateView<'_> {
        GateView {
            all: &self.all,
            any: &self.any,
            not_gate: &self.not_gate,
            contains: &self.contains,
            regex: &self.regex,
            line_regex: &self.line_regex,
        }
    }
}

impl GateSpec {
    fn gate(&self) -> GateView<'_> {
        GateView {
            all: &self.all,
            any: &self.any,
            not_gate: &self.not_gate,
            contains: &self.contains,
            regex: &self.regex,
            line_regex: &self.line_regex,
        }
    }
}

impl GateView<'_> {
    fn has_positive_matcher(&self) -> bool {
        !self.contains.is_empty()
            || !self.regex.is_empty()
            || !self.line_regex.is_empty()
            || !self.all.is_empty()
            || !self.any.is_empty()
    }

    fn has_any_matcher(&self) -> bool {
        self.has_positive_matcher() || !self.not_gate.is_empty()
    }
}

/// Every gate needs a positive matcher; a `not` gate only needs some matcher,
/// since it is only ever a condition on another gate.
fn validate_gate(
    gate: &GateView<'_>,
    context: &str,
    depth: usize,
    complexity: &mut Complexity,
) -> Result<(), String> {
    count_gate(gate, context, depth, complexity)?;
    if !gate.has_positive_matcher() {
        return Err(format!("{context} must contain a positive matcher"));
    }
    validate_children(gate, context, depth, complexity)
}

fn validate_not_gate(
    gate: &GateView<'_>,
    depth: usize,
    complexity: &mut Complexity,
) -> Result<(), String> {
    count_gate(gate, "not gate", depth, complexity)?;
    if !gate.has_any_matcher() {
        return Err("not gate must contain a matcher".to_string());
    }
    validate_children(gate, "not", depth, complexity)
}

fn count_gate(
    gate: &GateView<'_>,
    context: &str,
    depth: usize,
    complexity: &mut Complexity,
) -> Result<(), String> {
    if depth > MAX_GATE_DEPTH {
        return Err(format!("{context} exceeds max gate depth {MAX_GATE_DEPTH}"));
    }
    complexity.gates += 1;
    if complexity.gates > MAX_TOTAL_GATES {
        return Err(format!("manifest exceeds max gate count {MAX_TOTAL_GATES}"));
    }
    let matchers = gate.contains.len() + gate.regex.len() + gate.line_regex.len();
    if matchers > MAX_MATCHERS_PER_GATE {
        return Err(format!(
            "{context} has {matchers} direct matchers, max is {MAX_MATCHERS_PER_GATE}"
        ));
    }
    complexity.matchers += matchers;
    if complexity.matchers > MAX_TOTAL_MATCHERS {
        return Err(format!(
            "manifest exceeds max matcher count {MAX_TOTAL_MATCHERS}"
        ));
    }
    let values = gate
        .contains
        .iter()
        .chain(gate.regex)
        .chain(gate.line_regex);
    if values
        .into_iter()
        .any(|value| value.chars().count() > MAX_MATCHER_CHARS)
    {
        return Err(format!(
            "{context} matcher exceeds max length {MAX_MATCHER_CHARS}"
        ));
    }
    for (field, patterns) in [("regex", gate.regex), ("line_regex", gate.line_regex)] {
        for pattern in patterns {
            Regex::new(pattern).map_err(|error| {
                format!("{context} contains invalid {field} pattern {pattern:?}: {error}")
            })?;
        }
    }
    Ok(())
}

fn validate_children(
    gate: &GateView<'_>,
    context: &str,
    depth: usize,
    complexity: &mut Complexity,
) -> Result<(), String> {
    for nested in gate.all {
        validate_gate(
            &nested.gate(),
            &format!("{context} all gate"),
            depth + 1,
            complexity,
        )?;
    }
    for nested in gate.any {
        validate_gate(
            &nested.gate(),
            &format!("{context} any gate"),
            depth + 1,
            complexity,
        )?;
    }
    for nested in gate.not_gate {
        if !nested.gate().has_any_matcher() {
            return Err(format!("{context} contains an empty not gate"));
        }
        validate_not_gate(&nested.gate(), depth + 1, complexity)?;
    }
    Ok(())
}

#[derive(Debug)]
struct CompiledRule {
    id: String,
    state: AgentState,
    priority: i32,
    region: Region,
    skip_state_update: bool,
    gate: CompiledGate,
}

impl CompiledRule {
    fn compile(rule: &RuleSpec) -> Result<Self, String> {
        Ok(Self {
            id: rule.id.clone(),
            state: rule.state.map_or(AgentState::Unknown, AgentState::from),
            priority: rule.priority,
            region: Region::parse(&rule.region).ok_or("invalid region")?,
            skip_state_update: rule.skip_state_update,
            gate: CompiledGate::compile(&rule.gate())?,
        })
    }
}

#[derive(Debug)]
struct CompiledGate {
    all: Vec<CompiledGate>,
    any: Vec<CompiledGate>,
    not_gate: Vec<CompiledGate>,
    /// Lowercased: `contains` is case-insensitive.
    contains: Vec<String>,
    regex: Vec<Regex>,
    line_regex: Vec<Regex>,
}

impl CompiledGate {
    fn compile(gate: &GateView<'_>) -> Result<Self, String> {
        let nested = |gates: &[GateSpec]| {
            gates
                .iter()
                .map(|gate| Self::compile(&gate.gate()))
                .collect::<Result<Vec<_>, _>>()
        };
        let patterns = |patterns: &[String]| {
            patterns
                .iter()
                .map(|pattern| Regex::new(pattern).map_err(|error| error.to_string()))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            all: nested(gate.all)?,
            any: nested(gate.any)?,
            not_gate: nested(gate.not_gate)?,
            contains: gate
                .contains
                .iter()
                .map(|needle| needle.to_lowercase())
                .collect(),
            regex: patterns(gate.regex)?,
            line_regex: patterns(gate.line_regex)?,
        })
    }

    fn matches(&self, text: &str) -> bool {
        self.matches_with(text, &text.to_lowercase())
    }

    /// `contains` needles must all occur, case-insensitively; `regex` patterns
    /// must all match the text; each `line_regex` pattern must match some line;
    /// every `all` gate and at least one `any` gate must match; no `not` gate
    /// may.
    fn matches_with(&self, text: &str, lower: &str) -> bool {
        self.contains
            .iter()
            .all(|needle| lower.contains(needle.as_str()))
            && self.regex.iter().all(|regex| regex.is_match(text))
            && self
                .line_regex
                .iter()
                .all(|regex| text.lines().any(|line| regex.is_match(line)))
            && self.all.iter().all(|gate| gate.matches_with(text, lower))
            && (self.any.is_empty() || self.any.iter().any(|gate| gate.matches_with(text, lower)))
            && !self
                .not_gate
                .iter()
                .any(|gate| gate.matches_with(text, lower))
    }
}

/// The slice of the input a rule reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Region {
    /// The whole screen sample.
    WholeRecent,
    /// After the last Codex prompt line, or everything without one.
    AfterLastPromptMarker,
    /// Before the current Codex prompt, or everything without one.
    BeforeCurrentPromptMarker,
    /// Everything, or nothing while a current Codex prompt is drawn.
    WholeRecentWithoutCurrentPromptMarker,
    /// The last response marker line above the current Codex prompt.
    CurrentPromptBlockMarker,
    /// From that marker line to the end.
    AfterCurrentPromptBlockMarker,
    /// Between the prompt box's top border and the next rule.
    PromptBoxBody,
    /// Everything above the prompt box's top border.
    AbovePromptBox,
    /// The last non-empty line above the prompt box.
    LastNonEmptyAbovePromptBox,
    /// After the last horizontal rule, or everything without one.
    AfterLastHorizontalRule,
    /// The terminal title.
    OscTitle,
    /// The OSC 9;4 progress payload, which tiri does not capture.
    OscProgress,
    /// The last `n` lines.
    BottomLines(usize),
    /// From the `n`th-last non-empty line to the end.
    BottomNonEmptyLines(usize),
    /// From the start through the `n`th non-empty line.
    TopNonEmptyLines(usize),
}

impl Region {
    fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        Some(match spec {
            "whole_recent" => Self::WholeRecent,
            "after_last_prompt_marker" => Self::AfterLastPromptMarker,
            "before_current_prompt_marker" => Self::BeforeCurrentPromptMarker,
            "whole_recent_without_current_prompt_marker" => {
                Self::WholeRecentWithoutCurrentPromptMarker
            }
            "current_prompt_block_marker" => Self::CurrentPromptBlockMarker,
            "after_current_prompt_block_marker" => Self::AfterCurrentPromptBlockMarker,
            "prompt_box_body" => Self::PromptBoxBody,
            "above_prompt_box" => Self::AbovePromptBox,
            "last_non_empty_above_prompt_box" => Self::LastNonEmptyAbovePromptBox,
            "after_last_horizontal_rule" => Self::AfterLastHorizontalRule,
            "osc_title" => Self::OscTitle,
            "osc_progress" => Self::OscProgress,
            _ => {
                if let Some(count) = region_count(spec, "bottom_lines") {
                    Self::BottomLines(count)
                } else if let Some(count) = region_count(spec, "bottom_non_empty_lines") {
                    Self::BottomNonEmptyLines(count)
                } else {
                    Self::TopNonEmptyLines(top_region_count(spec)?)
                }
            }
        })
    }
}

fn region_count(spec: &str, name: &str) -> Option<usize> {
    spec.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')?
        .parse()
        .ok()
}

/// `top_non_empty_lines(n)` takes a canonical count: no sign, no leading zero,
/// and at most `u16::MAX`.
fn top_region_count(spec: &str) -> Option<usize> {
    let count = spec
        .strip_prefix("top_non_empty_lines")?
        .strip_prefix('(')?
        .strip_suffix(')')?;
    if count.starts_with('0') || !count.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    count
        .parse::<usize>()
        .ok()
        .filter(|count| *count <= MAX_TOP_REGION_LINE_COUNT)
}

fn region<'a>(input: Input<'a>, region: &Region) -> &'a str {
    let content = input.screen;
    match *region {
        Region::OscTitle => input.title,
        Region::OscProgress => "",
        Region::WholeRecent => content,
        Region::AfterLastPromptMarker => after_last_prompt_marker(content),
        Region::BeforeCurrentPromptMarker => before_current_prompt_marker(content),
        Region::WholeRecentWithoutCurrentPromptMarker => {
            whole_recent_without_current_prompt_marker(content)
        }
        Region::CurrentPromptBlockMarker => current_prompt_block_marker(content).unwrap_or(""),
        Region::AfterCurrentPromptBlockMarker => {
            after_current_prompt_block_marker(content).unwrap_or("")
        }
        Region::PromptBoxBody => prompt_box_body(content).unwrap_or(""),
        Region::AbovePromptBox => above_prompt_box(content),
        Region::LastNonEmptyAbovePromptBox => last_non_empty_line(above_prompt_box(content)),
        Region::AfterLastHorizontalRule => after_last_horizontal_rule(content),
        Region::BottomLines(count) => bottom_lines(content, count),
        Region::BottomNonEmptyLines(count) => bottom_non_empty_lines(content, count),
        Region::TopNonEmptyLines(count) => top_non_empty_lines(content, count),
    }
}

fn bottom_lines(content: &str, count: usize) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(count);
    slice_from_line(content, &lines, start)
}

fn bottom_non_empty_lines(content: &str, count: usize) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    let Some(start) = lines
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, line)| !line.trim().is_empty())
        .take(count)
        .last()
        .map(|(index, _)| index)
    else {
        return "";
    };
    slice_from_line(content, &lines, start)
}

fn top_non_empty_lines(content: &str, count: usize) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    let Some(end) = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .take(count)
        .last()
        .map(|(index, _)| index)
    else {
        return "";
    };
    &content[..line_start(content, &lines, end + 1)]
}

fn after_last_prompt_marker(content: &str) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    match lines.iter().rposition(|line| is_codex_prompt(line)) {
        Some(index) => slice_from_line(content, &lines, index + 1),
        None => content,
    }
}

fn before_current_prompt_marker(content: &str) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    match current_codex_prompt(&lines) {
        Some(index) => &content[..line_start(content, &lines, index)],
        None => content,
    }
}

fn whole_recent_without_current_prompt_marker(content: &str) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    if current_codex_prompt(&lines).is_some() {
        ""
    } else {
        content
    }
}

fn current_prompt_block_marker(content: &str) -> Option<&str> {
    let lines = content.lines().collect::<Vec<_>>();
    let prompt = current_codex_prompt(&lines)?;
    lines[..prompt]
        .iter()
        .rev()
        .find(|line| is_codex_block_marker(line))
        .copied()
}

fn after_current_prompt_block_marker(content: &str) -> Option<&str> {
    let lines = content.lines().collect::<Vec<_>>();
    let prompt = current_codex_prompt(&lines)?;
    let block = lines[..prompt]
        .iter()
        .rposition(|line| is_codex_block_marker(line))?;
    Some(slice_from_line(content, &lines, block))
}

/// The last Codex prompt line, unless a response marker follows it — a later
/// response makes that prompt stale.
fn current_codex_prompt(lines: &[&str]) -> Option<usize> {
    let prompt = lines.iter().rposition(|line| is_codex_prompt(line))?;
    if lines[prompt + 1..]
        .iter()
        .any(|line| is_codex_block_marker(line))
    {
        return None;
    }
    Some(prompt)
}

fn is_codex_prompt(line: &str) -> bool {
    line == "›" || line.starts_with("› ")
}

fn is_codex_block_marker(line: &str) -> bool {
    line.starts_with(['•', '■', '✗', '✓'])
}

fn prompt_box_body(content: &str) -> Option<&str> {
    let lines = content.lines().collect::<Vec<_>>();
    let top = prompt_box_top_border(&lines)?;
    let start = line_start(content, &lines, top + 1);
    let end = lines[top + 1..]
        .iter()
        .position(|line| is_horizontal_rule(line))
        .map_or(lines.len(), |relative| top + 1 + relative);
    Some(&content[start..line_start(content, &lines, end)])
}

fn above_prompt_box(content: &str) -> &str {
    let lines = content.lines().collect::<Vec<_>>();
    match prompt_box_top_border(&lines) {
        Some(top) => &content[..line_start(content, &lines, top)],
        None => content,
    }
}

fn after_last_horizontal_rule(content: &str) -> &str {
    let mut last_rule_end = 0;
    let mut offset = 0;
    for line in content.lines() {
        let next = offset + line.len() + 1;
        if is_horizontal_rule(line) {
            last_rule_end = next.min(content.len());
        }
        offset = next;
    }
    &content[last_rule_end..]
}

fn last_non_empty_line(content: &str) -> &str {
    content
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

/// The second-to-last horizontal rule: the top border of the box whose bottom
/// border is the last one.
fn prompt_box_top_border(lines: &[&str]) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, line)| is_horizontal_rule(line))
        .nth(1)
        .map(|(index, _)| index)
}

/// A line that starts with `─`: either nothing but `─`, or a run of at least
/// three followed by a label (`─── esc to cancel`).
fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    let rule = trimmed.chars().take_while(|&c| c == '─').count();
    if rule == 0 {
        return false;
    }
    let suffix = trimmed[rule * '─'.len_utf8()..].trim_start();
    suffix.is_empty() || rule >= 3
}

fn slice_from_line<'a>(content: &'a str, lines: &[&str], index: usize) -> &'a str {
    &content[line_start(content, lines, index)..]
}

/// Byte offset of line `index`, assuming `\n` separators as the screen sample
/// uses; clamped to the content.
fn line_start(content: &str, lines: &[&str], index: usize) -> usize {
    lines[..index.min(lines.len())]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .min(content.len())
}
