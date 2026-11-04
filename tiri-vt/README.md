# tiri-vt

Terminal protocol decoding and terminal identification, independent of tiri's
clients, screen storage, event loop, and terminfo implementation.

Application output is consumed by `Parser`, which calls `VtHost` to apply
terminal operations. Client terminal identification replies are consumed by
`parse_terminal_reply`, which uses the two-method `TerminalHost` trait:

```rust,ignore
trait TerminalHost {
    fn terminal_reply_state(&self) -> TerminalReplyState;
    fn terminal_report(&mut self, report: TerminalReport<'_>);
}
```

The host supplies which replies have already arrived and whether synchronized
output is supported, including capabilities obtained from terminfo. The decoder
recognizes DA1, DA2, XTVERSION, and synchronized-output mode reports. It translates
protocol fields into features, terminal identity, and xterm.js detection updates.
The host applies the report and records its reply kind as received.

A successful parse returns the number of consumed bytes. `Incomplete` consumes
nothing and publishes nothing; retain the bytes and retry when more arrive.
`Unrecognized` also publishes nothing and lets the caller try another input
protocol. Input after a complete reply belongs to the caller. Duplicate reply
kinds are rejected using the host's state.

The two host traits describe opposite protocol directions. Neither owns client
connections or rendering policy. Tiri applies detected capabilities, configuration
overrides, and rendering workarounds. Terminal query scheduling, terminfo, keyboard
and mouse decoding, and colour/clipboard replies currently remain in tiri.
