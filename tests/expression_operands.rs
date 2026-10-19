//! `#{e|...}` reads each operand as C `strtod` does: the longest numeric
//! prefix, correctly rounded, with anything after it making the expression
//! empty.

#![cfg(unix)]

mod common;

use common::Server;

fn server() -> Server {
    let server = Server::new();
    server.success(&["new-session", "-d", "sleep 60"]);
    server
}

#[test]
fn decimal_and_special_operands_follow_strtod() {
    let server = server();
    for (format, expected) in [
        ("#{e|+|f|6: 1, +2}", "3.000000"),
        ("#{e|+|f|6:.5,5.}", "5.500000"),
        ("#{e|+|f|6:1e,2}", ""),
        ("#{e|+|f|6:1e+,2}", ""),
        ("#{e|+|f|6:1x,1}", ""),
        ("#{e|+|:,1}", "1"),
        ("#{e|+|f|6:inf,1}", "inf"),
        ("#{e|+|f|6:INFINITY,1}", "inf"),
        ("#{e|+|f|6:infx,1}", ""),
        ("#{e|+|f|6:nan(123),1}", "nan"),
        ("#{e|+|f|6:1e400,1}", "inf"),
        ("#{e|+|:9007199254740993,0}", "9007199254740992"),
        (
            "#{e|+|:123456789012345678901234567890,0}",
            "-9223372036854775808",
        ),
    ] {
        assert_eq!(server.display(format), expected, "{format}");
    }
}

#[test]
fn narrow_hex_operands_follow_strtod() {
    let server = server();
    for (format, expected) in [
        ("#{e|+|f|6:0x1.8p1,0x.8}", "3.500000"),
        ("#{e|+|f|6:0X1P-2,0xAbC}", "2748.250000"),
        ("#{e|+|f|6:0x1.,1}", "2.000000"),
        ("#{e|+|f|6:-0x10,1}", "-15.000000"),
        ("#{e|+|f|6:0x,1}", ""),
        ("#{e|+|f|6:0x1p,1}", ""),
        ("#{e|+|:0x1fffffffffffff,0}", "9007199254740991"),
        ("#{e|+|:0x20000000000001,0}", "9007199254740992"),
        ("#{e|+|:0x20000000000003,0}", "9007199254740996"),
        (
            "#{e|*|f|30:0x1p-1074,1e300}",
            "0.000000000000000000000004940656",
        ),
    ] {
        assert_eq!(server.display(format), expected, "{format}");
    }
}

/// A mantissa wider than a double rounds once over all its digits, and a
/// long one is scaled by its binary exponent before it can overflow or
/// underflow.
#[test]
fn wide_hex_operands_round_once() {
    let server = server();
    let zeros = "0".repeat(256);
    let places = "0".repeat(300);
    for (format, expected) in [
        // 2^57 + 17 rounds to 2^57 + 32, not to 2^57 via 2^53 + 1.
        (
            "#{e|+|:0x200000000000011,0}".to_owned(),
            "144115188075855904",
        ),
        (
            "#{e|+|f|20:0x1.000000000000081,0}".to_owned(),
            "1.00000000000000022204",
        ),
        (format!("#{{e|+|:0x1{zeros}p-1024,0}}"), "1"),
        (
            format!("#{{e|+|f|3:0x0.{places}1p1300,0}}"),
            "79228162514264337593543950336.000",
        ),
    ] {
        assert_eq!(server.display(&format), expected, "{format}");
    }
}
