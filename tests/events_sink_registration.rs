use tiri::src::events::{events_add_sink, events_fire, events_remove_sink};
use tiri::src::events_payload::event_payload_create;
use tiri::src::shared::events::{events_callback, EventSinkId};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn sink_ids_survive_self_removal_and_do_not_remove_replacements() {
    unsafe {
        let first_id = Rc::new(Cell::new(EventSinkId::default()));
        let second_id = Rc::new(Cell::new(EventSinkId::default()));
        let first_calls = Rc::new(Cell::new(0));
        let second_calls = Rc::new(Cell::new(0));
        let name = c"sink-id-lifecycle";

        let self_id = first_id.clone();
        let replacement_id = second_id.clone();
        let first_count = first_calls.clone();
        let second_count = second_calls.clone();
        first_id.set(events_add_sink(
            name,
            events_callback(move |_, _| {
                first_count.set(first_count.get() + 1);
                events_remove_sink(self_id.get());
                let count = second_count.clone();
                let id = events_add_sink(
                    c"sink-id-lifecycle",
                    events_callback(move |_, _| count.set(count.get() + 1)),
                );
                replacement_id.set(id);
            }),
        ));

        events_fire(name, event_payload_create());
        assert_eq!(first_calls.get(), 1);
        assert_eq!(second_calls.get(), 0);
        assert_ne!(first_id.get(), second_id.get());

        events_remove_sink(first_id.get());
        events_fire(name, event_payload_create());
        assert_eq!(first_calls.get(), 1);
        assert_eq!(second_calls.get(), 1);

        events_remove_sink(second_id.get());
        events_remove_sink(EventSinkId::default());
        events_fire(name, event_payload_create());
        assert_eq!(second_calls.get(), 1);
    }
}

#[test]
fn firing_with_logging_enabled_records_the_name_and_every_payload_item() {
    use tiri::src::events_payload::{event_payload_set_int, event_payload_set_string};
    use tiri::src::log::{log_add_level, log_get_level, log_open, log_toggle};

    // The server log is written to the working directory; keep it private.
    let directory = std::env::temp_dir().join(format!(
        "tiri-events-log-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(&directory).unwrap();

    let delivered = Rc::new(Cell::new(0));
    unsafe {
        assert_eq!(log_get_level(), 0);
        log_add_level();
        log_open(c"events");

        let count = delivered.clone();
        let id = events_add_sink(
            c"@logged-event",
            events_callback(move |_, _| count.set(count.get() + 1)),
        );
        let mut payload = event_payload_create();
        event_payload_set_string(&mut payload, c"value", |out| out.write_all(b"a\tb"));
        event_payload_set_int(&mut payload, c"window_index", -3);
        events_fire(c"@logged-event", payload);
        events_remove_sink(id);

        // Toggling an enabled log closes it and turns logging off again.
        log_toggle(c"events");
        assert_eq!(log_get_level(), 0);
    }
    std::env::set_current_dir(previous).unwrap();

    let path = directory.join(format!("tmux-events-{}.log", std::process::id()));
    let log = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();

    // Logging does not change delivery.
    assert_eq!(delivered.get(), 1);
    // The line names the fired event, then lists every item by name, including
    // the event name the dispatcher adds itself; the logger escapes control
    // characters.
    let line = log
        .lines()
        .find(|line| line.contains("events_fire: "))
        .unwrap_or_else(|| panic!("no events_fire line in {log:?}"));
    assert!(
        line.ends_with(
            "events_fire: @logged-event: event=@logged-event, value=a\\tb, window_index=-3"
        ),
        "unexpected events_fire log line: {line:?}"
    );
}
