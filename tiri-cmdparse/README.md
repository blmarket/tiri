# tiri-cmdparse

The LALRPOP grammar and owned syntax tree extracted from `tiri/src/cmd/parse.rs`
and `parse_grammar.lalrpop`. The grammar productions are preserved; application
services are supplied through `Context`.

`parse(context, tokens)` accepts a lazy stream of located tokens and returns owned
commands, including nested command arguments. The caller supplies tokenization,
format expansion, truth evaluation, assignment validation and environment updates.
The lexer must emit a newline after the final statement. Keep tokenization lazy:
assignments performed during parsing can affect expansion in subsequent tokens.
Token strings use `CString` to preserve non-UTF-8 command bytes.

The tiri adapter lives in `src/cmd/parse.rs`, with a custom byte-oriented
`Lexer` in `src/cmd/parse/lexer.rs`. The lexer yields typed tokens and reports
structured errors; each parse owns its scanner state and diagnostic. Grammar
actions and the lazy lexer share a scoped parsing session. Alias expansion and
command construction remain application-owned. Environment and other application
services still use global state, so this does not make parsing thread-safe.

Run `cargo test -p tiri-cmdparse` for standalone grammar tests.
