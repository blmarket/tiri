use super::{Bundle, Input, Region, Rules, region};
use crate::integration::{AgentState, Detection, agy, claude, codex, opencode, pi};

fn input(screen: &str) -> Input<'_> {
    Input { screen, title: "" }
}

/// Rules for a test agent: `base` under `overlay`.
fn rules(base: &str, overlay: &str) -> Result<Rules, String> {
    Rules::load(
        &Bundle {
            agent: "test",
            herdr: base,
            tiri: overlay,
        },
        None,
    )
}

const BASE: &str = r#"
id = "test"
version = "1"

[[rules]]
id = "low_contains"
state = "idle"
priority = 1
contains = ["MATCH"]

[[rules]]
id = "high_nested_gates"
state = "working"
priority = 10
contains = ["match"]
all = [
  { any = [{ regex = ["w[io]n"] }, { contains = ["fallback"] }] },
]
not = [
  { contains = ["blocked"] },
]

[[rules]]
id = "line_regex"
state = "blocked"
priority = 20
line_regex = ["^exact line$"]
"#;

const EMPTY_OVERLAY: &str = r#"id = "test""#;

#[test]
fn rules_apply_gates_priority_and_line_regex() {
    let rules = rules(BASE, EMPTY_OVERLAY).expect("rules");
    assert_eq!(
        rules.matched_rule(input("match win")),
        Some("high_nested_gates")
    );
    assert_eq!(
        rules.matched_rule(input("match fallback")),
        Some("high_nested_gates")
    );
    assert_eq!(
        rules.matched_rule(input("match win blocked")),
        Some("low_contains")
    );
    assert_eq!(
        rules.matched_rule(input("before\nexact line\nafter")),
        Some("line_regex")
    );
    assert_eq!(rules.matched_rule(input("exact line after")), None);
    assert_eq!(
        rules.detect(input("nothing")),
        Detection::State(AgentState::Unknown)
    );
}

#[test]
fn equal_priorities_go_to_the_rule_listed_first() {
    let rules = rules(
        r#"
id = "test"

[[rules]]
id = "first"
state = "idle"
priority = 5
contains = ["x"]

[[rules]]
id = "second"
state = "working"
priority = 5
contains = ["x"]
"#,
        EMPTY_OVERLAY,
    )
    .expect("rules");
    assert_eq!(rules.matched_rule(input("x")), Some("first"));
}

#[test]
fn title_rules_read_the_title_and_skip_rules_keep_the_previous_state() {
    let rules = rules(
        r#"
id = "test"

[[rules]]
id = "title"
state = "working"
priority = 10
region = "osc_title"
contains = ["busy"]

[[rules]]
id = "viewer"
state = "unknown"
priority = 5
skip_state_update = true
contains = ["viewer"]

[[rules]]
id = "progress"
state = "idle"
priority = 20
region = "osc_progress"
regex = ['^4;0']
"#,
        EMPTY_OVERLAY,
    )
    .expect("rules");
    let busy = Input {
        screen: "viewer",
        title: "busy",
    };
    assert_eq!(rules.detect(busy), Detection::State(AgentState::Working));
    assert_eq!(rules.detect(input("viewer")), Detection::KeepPrevious);
    assert_eq!(rules.matched_rule(input("4;0")), None);
}

#[test]
fn an_overlay_replaces_adds_and_disables_rules() {
    let overlay = r#"
id = "test"
version = "1"
disable = ["line_regex"]

[[rules]]
id = "low_contains"
state = "blocked"
priority = 1
contains = ["match"]

[[rules]]
id = "added"
state = "working"
priority = 30
contains = ["extra"]
"#;
    let rules = rules(BASE, overlay).expect("rules");
    assert_eq!(
        rules.detect(input("match")),
        Detection::State(AgentState::Blocked)
    );
    assert_eq!(rules.matched_rule(input("extra match win")), Some("added"));
    assert_eq!(rules.matched_rule(input("exact line")), None);
}

#[test]
fn an_overlay_must_name_existing_rules_and_its_reviewed_version() {
    let unknown = r#"
id = "test"
disable = ["renamed_upstream"]
"#;
    let error = rules(BASE, unknown).expect_err("disabling a missing rule");
    assert!(error.contains("renamed_upstream"), "{error}");

    let stale = r#"
id = "test"
version = "0"
"#;
    let error = rules(BASE, stale).expect_err("overlay for another version");
    assert!(error.contains("version 0"), "{error}");

    let other = r#"id = "other""#;
    assert!(rules(BASE, other).is_err());
}

#[test]
fn only_an_overlay_may_disable_rules() {
    let base = r#"
id = "test"
disable = ["x"]

[[rules]]
id = "x"
state = "idle"
contains = ["x"]
"#;
    assert!(rules(base, EMPTY_OVERLAY).is_err());
}

#[test]
fn validation_rejects_malformed_manifests() {
    let invalid = [
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\ncontains = [\"x\"]\nbogus = 1\n",
        "id = \"test\"\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nregion = \"middle\"\ncontains = [\"x\"]\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nregex = [\"(\"]\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nnot = [{ contains = [\"x\"] }]\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nskip_state_update = true\ncontains = [\"x\"]\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"unknown\"\nskip_state_update = true\nvisible_idle = true\ncontains = [\"x\"]\n",
        "id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\ncontains = [\"x\"]\n[[rules]]\nid = \"a\"\nstate = \"idle\"\ncontains = [\"y\"]\n",
        "id = \"test\"\nmin_engine_version = 4\n[[rules]]\nid = \"a\"\nstate = \"idle\"\ncontains = [\"x\"]\n",
        "id = \"test\"\nmin_engine_version = 2\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nregion = \"top_non_empty_lines(1)\"\ncontains = [\"x\"]\n",
    ];
    for manifest in invalid {
        assert!(
            rules(manifest, EMPTY_OVERLAY).is_err(),
            "accepted:\n{manifest}"
        );
    }
}

#[test]
fn validation_bounds_gate_depth_and_matcher_count() {
    let mut deep = String::from("{ contains = [\"x\"] }");
    for _ in 0..10 {
        deep = format!("{{ all = [{deep}] }}");
    }
    let manifest =
        format!("id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\nall = [{deep}]\n");
    assert!(rules(&manifest, EMPTY_OVERLAY).is_err());

    let needles = (0..33)
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest =
        format!("id = \"test\"\n[[rules]]\nid = \"a\"\nstate = \"idle\"\ncontains = [{needles}]\n");
    assert!(rules(&manifest, EMPTY_OVERLAY).is_err());
}

#[test]
fn regions_extract_structure() {
    for (screen, spec, expected) in [
        ("old\n\nnew\n", "bottom_lines(2)", "\nnew\n"),
        (
            "before\n› input\nafter\n",
            "after_last_prompt_marker",
            "after\n",
        ),
        (
            "before\n› input\nafter\n",
            "before_current_prompt_marker",
            "before\n",
        ),
        (
            "before\n› input\nafter\n",
            "whole_recent_without_current_prompt_marker",
            "",
        ),
        (
            "no marker\n",
            "whole_recent_without_current_prompt_marker",
            "no marker\n",
        ),
        (
            "• old\n■ latest\n› input\n",
            "current_prompt_block_marker",
            "■ latest",
        ),
        (
            "• old\n■ latest\n› input\n",
            "after_current_prompt_block_marker",
            "■ latest\n› input\n",
        ),
        ("› old\n• new\n", "current_prompt_block_marker", ""),
        (
            "above\n\n───\nbody\n───\nfooter\n",
            "above_prompt_box",
            "above\n\n",
        ),
        (
            "above\n\n───\nbody\n───\nfooter\n",
            "last_non_empty_above_prompt_box",
            "above",
        ),
        (
            "above\n───\nbody\n───\nfooter\n",
            "prompt_box_body",
            "body\n",
        ),
        ("only\n───\nrule\n", "prompt_box_body", ""),
        (
            "above\n───\nbody\n───\nfooter\n",
            "after_last_horizontal_rule",
            "footer\n",
        ),
        (
            "body\n─── esc to cancel\nfooter",
            "after_last_horizontal_rule",
            "footer",
        ),
        (
            "marker\nold\n\nmiddle\nmarker\nnew\n",
            "bottom_non_empty_lines(2)",
            "marker\nnew\n",
        ),
        (
            "\nmarker\nold\n\nmiddle\nmarker\nnew\n",
            "top_non_empty_lines(2)",
            "\nmarker\nold\n",
        ),
    ] {
        let region_spec = Region::parse(spec).expect(spec);
        assert_eq!(
            region(input(screen), &region_spec),
            expected,
            "region={spec}"
        );
    }
}

#[test]
fn top_non_empty_lines_takes_a_canonical_bounded_count() {
    assert!(Region::parse("top_non_empty_lines(1)").is_some());
    assert!(Region::parse(&format!("top_non_empty_lines({})", u16::MAX)).is_some());
    for count in ["0", "01", "+1", "65536", "999999999999999999999999"] {
        assert!(
            Region::parse(&format!("top_non_empty_lines({count})")).is_none(),
            "accepted {count}"
        );
    }
}

/// Every bundled overlay was reviewed against the herdr manifest it sits on;
/// updating a herdr manifest without reviewing its overlay fails here.
#[test]
fn bundled_rules_load() {
    for bundle in [
        &claude::RULES,
        &codex::RULES,
        &pi::RULES,
        &agy::RULES,
        &opencode::RULES,
    ] {
        if let Err(error) = Rules::load(bundle, None) {
            panic!("{} rules: {error}", bundle.agent);
        }
    }
}

#[test]
fn a_user_overlay_layers_over_the_bundled_rules() {
    let overlay = r#"
id = "claude"
disable = ["live_prompt_box"]

[[rules]]
id = "my_footer"
state = "blocked"
priority = 2000
contains = ["needs me"]
"#;
    let rules = Rules::load(&claude::RULES, Some(overlay)).expect("rules");
    assert_eq!(
        rules.detect(input("needs me")),
        Detection::State(AgentState::Blocked)
    );
    assert!(Rules::load(&claude::RULES, Some("id = \"codex\"")).is_err());
}
