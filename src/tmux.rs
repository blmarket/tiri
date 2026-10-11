#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_assignments,
    unused_mut
)]
use crate::src::cfg::{cfg_quiet, cfg_set_files};
use crate::src::client::client_main;
use crate::src::compat::fdforkpty::getptmfd;
use crate::src::compat::getopt_long::{BSDgetopt, BSDoptarg, BSDoptind};
use crate::src::compat::getprogname::getprogname;
use crate::src::environ::{environ_create, environ_find, environ_put, environ_set};
use crate::src::ffi::libc::nl_item;
use crate::src::ffi::libc::{
    __errno_location, access, clock_gettime, environ, err, errx, exit, fprintf, getcwd, getenv,
    getpwuid, getuid, lstat, mkdir, nl_langinfo, printf, realpath, setlocale, stderr, stdout,
    strcasecmp, strcasestr, strcmp, strerror, strrchr, strsep, strstr, tzset,
};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{log_add_level, log_cstr, log_debug};
use crate::src::options::{
    options_create, options_default, options_set_number, options_set_string,
};
use crate::src::options_table::options_table;
use crate::src::shared::abi::*;
use crate::src::shared::abi::{__mode_t, uid_t};
use crate::src::shared::account::passwd;
use crate::src::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROLCONTROL, CLIENT_DEFAULTSOCKET, CLIENT_LOGIN, CLIENT_NOFORK,
    CLIENT_NOSTARTSERVER, CLIENT_UTF8,
};
use crate::src::shared::environment::{environ, environ_entry};
use crate::src::shared::key::{MODEKEY_EMACS, MODEKEY_VI};
use crate::src::shared::options::options;
use crate::src::shared::options::{
    OPTIONS_TABLE_SERVER, OPTIONS_TABLE_SESSION, OPTIONS_TABLE_WINDOW,
};
use crate::src::shared::posix_io::stat;
use crate::src::shared::posix_io::{S_IRWXU, X_OK};
use crate::src::shared::time::{timespec, CLOCK_REALTIME};
use crate::src::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::src::text::utf8::{utf8_isvalid, utf8_stravis_cstring};
use crate::src::tty_features::tty_parse_features;
use std::ffi::{CStr, CString};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub type C2RustUnnamed = ::core::ffi::c_uint;
pub const _NL_NUM: C2RustUnnamed = 786449;
pub const _NL_NUM_LC_IDENTIFICATION: C2RustUnnamed = 786448;
pub const _NL_IDENTIFICATION_CODESET: C2RustUnnamed = 786447;
pub const _NL_IDENTIFICATION_CATEGORY: C2RustUnnamed = 786446;
pub const _NL_IDENTIFICATION_DATE: C2RustUnnamed = 786445;
pub const _NL_IDENTIFICATION_REVISION: C2RustUnnamed = 786444;
pub const _NL_IDENTIFICATION_ABBREVIATION: C2RustUnnamed = 786443;
pub const _NL_IDENTIFICATION_APPLICATION: C2RustUnnamed = 786442;
pub const _NL_IDENTIFICATION_AUDIENCE: C2RustUnnamed = 786441;
pub const _NL_IDENTIFICATION_TERRITORY: C2RustUnnamed = 786440;
pub const _NL_IDENTIFICATION_LANGUAGE: C2RustUnnamed = 786439;
pub const _NL_IDENTIFICATION_FAX: C2RustUnnamed = 786438;
pub const _NL_IDENTIFICATION_TEL: C2RustUnnamed = 786437;
pub const _NL_IDENTIFICATION_EMAIL: C2RustUnnamed = 786436;
pub const _NL_IDENTIFICATION_CONTACT: C2RustUnnamed = 786435;
pub const _NL_IDENTIFICATION_ADDRESS: C2RustUnnamed = 786434;
pub const _NL_IDENTIFICATION_SOURCE: C2RustUnnamed = 786433;
pub const _NL_IDENTIFICATION_TITLE: C2RustUnnamed = 786432;
pub const _NL_NUM_LC_MEASUREMENT: C2RustUnnamed = 720898;
pub const _NL_MEASUREMENT_CODESET: C2RustUnnamed = 720897;
pub const _NL_MEASUREMENT_MEASUREMENT: C2RustUnnamed = 720896;
pub const _NL_NUM_LC_TELEPHONE: C2RustUnnamed = 655365;
pub const _NL_TELEPHONE_CODESET: C2RustUnnamed = 655364;
pub const _NL_TELEPHONE_INT_PREFIX: C2RustUnnamed = 655363;
pub const _NL_TELEPHONE_INT_SELECT: C2RustUnnamed = 655362;
pub const _NL_TELEPHONE_TEL_DOM_FMT: C2RustUnnamed = 655361;
pub const _NL_TELEPHONE_TEL_INT_FMT: C2RustUnnamed = 655360;
pub const _NL_NUM_LC_ADDRESS: C2RustUnnamed = 589837;
pub const _NL_ADDRESS_CODESET: C2RustUnnamed = 589836;
pub const _NL_ADDRESS_LANG_LIB: C2RustUnnamed = 589835;
pub const _NL_ADDRESS_LANG_TERM: C2RustUnnamed = 589834;
pub const _NL_ADDRESS_LANG_AB: C2RustUnnamed = 589833;
pub const _NL_ADDRESS_LANG_NAME: C2RustUnnamed = 589832;
pub const _NL_ADDRESS_COUNTRY_ISBN: C2RustUnnamed = 589831;
pub const _NL_ADDRESS_COUNTRY_NUM: C2RustUnnamed = 589830;
pub const _NL_ADDRESS_COUNTRY_CAR: C2RustUnnamed = 589829;
pub const _NL_ADDRESS_COUNTRY_AB3: C2RustUnnamed = 589828;
pub const _NL_ADDRESS_COUNTRY_AB2: C2RustUnnamed = 589827;
pub const _NL_ADDRESS_COUNTRY_POST: C2RustUnnamed = 589826;
pub const _NL_ADDRESS_COUNTRY_NAME: C2RustUnnamed = 589825;
pub const _NL_ADDRESS_POSTAL_FMT: C2RustUnnamed = 589824;
pub const _NL_NUM_LC_NAME: C2RustUnnamed = 524295;
pub const _NL_NAME_CODESET: C2RustUnnamed = 524294;
pub const _NL_NAME_NAME_MS: C2RustUnnamed = 524293;
pub const _NL_NAME_NAME_MISS: C2RustUnnamed = 524292;
pub const _NL_NAME_NAME_MRS: C2RustUnnamed = 524291;
pub const _NL_NAME_NAME_MR: C2RustUnnamed = 524290;
pub const _NL_NAME_NAME_GEN: C2RustUnnamed = 524289;
pub const _NL_NAME_NAME_FMT: C2RustUnnamed = 524288;
pub const _NL_NUM_LC_PAPER: C2RustUnnamed = 458755;
pub const _NL_PAPER_CODESET: C2RustUnnamed = 458754;
pub const _NL_PAPER_WIDTH: C2RustUnnamed = 458753;
pub const _NL_PAPER_HEIGHT: C2RustUnnamed = 458752;
pub const _NL_NUM_LC_MESSAGES: C2RustUnnamed = 327685;
pub const _NL_MESSAGES_CODESET: C2RustUnnamed = 327684;
pub const __NOSTR: C2RustUnnamed = 327683;
pub const __YESSTR: C2RustUnnamed = 327682;
pub const __NOEXPR: C2RustUnnamed = 327681;
pub const __YESEXPR: C2RustUnnamed = 327680;
pub const _NL_NUM_LC_NUMERIC: C2RustUnnamed = 65542;
pub const _NL_NUMERIC_CODESET: C2RustUnnamed = 65541;
pub const _NL_NUMERIC_THOUSANDS_SEP_WC: C2RustUnnamed = 65540;
pub const _NL_NUMERIC_DECIMAL_POINT_WC: C2RustUnnamed = 65539;
pub const __GROUPING: C2RustUnnamed = 65538;
pub const __THOUSANDS_SEP: C2RustUnnamed = 65537;
pub const __DECIMAL_POINT: C2RustUnnamed = 65536;
pub const _NL_NUM_LC_MONETARY: C2RustUnnamed = 262190;
pub const _NL_MONETARY_CODESET: C2RustUnnamed = 262189;
pub const _NL_MONETARY_THOUSANDS_SEP_WC: C2RustUnnamed = 262188;
pub const _NL_MONETARY_DECIMAL_POINT_WC: C2RustUnnamed = 262187;
pub const _NL_MONETARY_CONVERSION_RATE: C2RustUnnamed = 262186;
pub const _NL_MONETARY_DUO_VALID_TO: C2RustUnnamed = 262185;
pub const _NL_MONETARY_DUO_VALID_FROM: C2RustUnnamed = 262184;
pub const _NL_MONETARY_UNO_VALID_TO: C2RustUnnamed = 262183;
pub const _NL_MONETARY_UNO_VALID_FROM: C2RustUnnamed = 262182;
pub const _NL_MONETARY_DUO_INT_N_SIGN_POSN: C2RustUnnamed = 262181;
pub const _NL_MONETARY_DUO_INT_P_SIGN_POSN: C2RustUnnamed = 262180;
pub const _NL_MONETARY_DUO_N_SIGN_POSN: C2RustUnnamed = 262179;
pub const _NL_MONETARY_DUO_P_SIGN_POSN: C2RustUnnamed = 262178;
pub const _NL_MONETARY_DUO_INT_N_SEP_BY_SPACE: C2RustUnnamed = 262177;
pub const _NL_MONETARY_DUO_INT_N_CS_PRECEDES: C2RustUnnamed = 262176;
pub const _NL_MONETARY_DUO_INT_P_SEP_BY_SPACE: C2RustUnnamed = 262175;
pub const _NL_MONETARY_DUO_INT_P_CS_PRECEDES: C2RustUnnamed = 262174;
pub const _NL_MONETARY_DUO_N_SEP_BY_SPACE: C2RustUnnamed = 262173;
pub const _NL_MONETARY_DUO_N_CS_PRECEDES: C2RustUnnamed = 262172;
pub const _NL_MONETARY_DUO_P_SEP_BY_SPACE: C2RustUnnamed = 262171;
pub const _NL_MONETARY_DUO_P_CS_PRECEDES: C2RustUnnamed = 262170;
pub const _NL_MONETARY_DUO_FRAC_DIGITS: C2RustUnnamed = 262169;
pub const _NL_MONETARY_DUO_INT_FRAC_DIGITS: C2RustUnnamed = 262168;
pub const _NL_MONETARY_DUO_CURRENCY_SYMBOL: C2RustUnnamed = 262167;
pub const _NL_MONETARY_DUO_INT_CURR_SYMBOL: C2RustUnnamed = 262166;
pub const __INT_N_SIGN_POSN: C2RustUnnamed = 262165;
pub const __INT_P_SIGN_POSN: C2RustUnnamed = 262164;
pub const __INT_N_SEP_BY_SPACE: C2RustUnnamed = 262163;
pub const __INT_N_CS_PRECEDES: C2RustUnnamed = 262162;
pub const __INT_P_SEP_BY_SPACE: C2RustUnnamed = 262161;
pub const __INT_P_CS_PRECEDES: C2RustUnnamed = 262160;
pub const _NL_MONETARY_CRNCYSTR: C2RustUnnamed = 262159;
pub const __N_SIGN_POSN: C2RustUnnamed = 262158;
pub const __P_SIGN_POSN: C2RustUnnamed = 262157;
pub const __N_SEP_BY_SPACE: C2RustUnnamed = 262156;
pub const __N_CS_PRECEDES: C2RustUnnamed = 262155;
pub const __P_SEP_BY_SPACE: C2RustUnnamed = 262154;
pub const __P_CS_PRECEDES: C2RustUnnamed = 262153;
pub const __FRAC_DIGITS: C2RustUnnamed = 262152;
pub const __INT_FRAC_DIGITS: C2RustUnnamed = 262151;
pub const __NEGATIVE_SIGN: C2RustUnnamed = 262150;
pub const __POSITIVE_SIGN: C2RustUnnamed = 262149;
pub const __MON_GROUPING: C2RustUnnamed = 262148;
pub const __MON_THOUSANDS_SEP: C2RustUnnamed = 262147;
pub const __MON_DECIMAL_POINT: C2RustUnnamed = 262146;
pub const __CURRENCY_SYMBOL: C2RustUnnamed = 262145;
pub const __INT_CURR_SYMBOL: C2RustUnnamed = 262144;
pub const _NL_NUM_LC_CTYPE: C2RustUnnamed = 86;
pub const _NL_CTYPE_EXTRA_MAP_14: C2RustUnnamed = 85;
pub const _NL_CTYPE_EXTRA_MAP_13: C2RustUnnamed = 84;
pub const _NL_CTYPE_EXTRA_MAP_12: C2RustUnnamed = 83;
pub const _NL_CTYPE_EXTRA_MAP_11: C2RustUnnamed = 82;
pub const _NL_CTYPE_EXTRA_MAP_10: C2RustUnnamed = 81;
pub const _NL_CTYPE_EXTRA_MAP_9: C2RustUnnamed = 80;
pub const _NL_CTYPE_EXTRA_MAP_8: C2RustUnnamed = 79;
pub const _NL_CTYPE_EXTRA_MAP_7: C2RustUnnamed = 78;
pub const _NL_CTYPE_EXTRA_MAP_6: C2RustUnnamed = 77;
pub const _NL_CTYPE_EXTRA_MAP_5: C2RustUnnamed = 76;
pub const _NL_CTYPE_EXTRA_MAP_4: C2RustUnnamed = 75;
pub const _NL_CTYPE_EXTRA_MAP_3: C2RustUnnamed = 74;
pub const _NL_CTYPE_EXTRA_MAP_2: C2RustUnnamed = 73;
pub const _NL_CTYPE_EXTRA_MAP_1: C2RustUnnamed = 72;
pub const _NL_CTYPE_NONASCII_CASE: C2RustUnnamed = 71;
pub const _NL_CTYPE_MAP_TO_NONASCII: C2RustUnnamed = 70;
pub const _NL_CTYPE_TRANSLIT_IGNORE: C2RustUnnamed = 69;
pub const _NL_CTYPE_TRANSLIT_IGNORE_LEN: C2RustUnnamed = 68;
pub const _NL_CTYPE_TRANSLIT_DEFAULT_MISSING: C2RustUnnamed = 67;
pub const _NL_CTYPE_TRANSLIT_DEFAULT_MISSING_LEN: C2RustUnnamed = 66;
pub const _NL_CTYPE_TRANSLIT_TO_TBL: C2RustUnnamed = 65;
pub const _NL_CTYPE_TRANSLIT_TO_IDX: C2RustUnnamed = 64;
pub const _NL_CTYPE_TRANSLIT_FROM_TBL: C2RustUnnamed = 63;
pub const _NL_CTYPE_TRANSLIT_FROM_IDX: C2RustUnnamed = 62;
pub const _NL_CTYPE_TRANSLIT_TAB_SIZE: C2RustUnnamed = 61;
pub const _NL_CTYPE_OUTDIGIT9_WC: C2RustUnnamed = 60;
pub const _NL_CTYPE_OUTDIGIT8_WC: C2RustUnnamed = 59;
pub const _NL_CTYPE_OUTDIGIT7_WC: C2RustUnnamed = 58;
pub const _NL_CTYPE_OUTDIGIT6_WC: C2RustUnnamed = 57;
pub const _NL_CTYPE_OUTDIGIT5_WC: C2RustUnnamed = 56;
pub const _NL_CTYPE_OUTDIGIT4_WC: C2RustUnnamed = 55;
pub const _NL_CTYPE_OUTDIGIT3_WC: C2RustUnnamed = 54;
pub const _NL_CTYPE_OUTDIGIT2_WC: C2RustUnnamed = 53;
pub const _NL_CTYPE_OUTDIGIT1_WC: C2RustUnnamed = 52;
pub const _NL_CTYPE_OUTDIGIT0_WC: C2RustUnnamed = 51;
pub const _NL_CTYPE_OUTDIGIT9_MB: C2RustUnnamed = 50;
pub const _NL_CTYPE_OUTDIGIT8_MB: C2RustUnnamed = 49;
pub const _NL_CTYPE_OUTDIGIT7_MB: C2RustUnnamed = 48;
pub const _NL_CTYPE_OUTDIGIT6_MB: C2RustUnnamed = 47;
pub const _NL_CTYPE_OUTDIGIT5_MB: C2RustUnnamed = 46;
pub const _NL_CTYPE_OUTDIGIT4_MB: C2RustUnnamed = 45;
pub const _NL_CTYPE_OUTDIGIT3_MB: C2RustUnnamed = 44;
pub const _NL_CTYPE_OUTDIGIT2_MB: C2RustUnnamed = 43;
pub const _NL_CTYPE_OUTDIGIT1_MB: C2RustUnnamed = 42;
pub const _NL_CTYPE_OUTDIGIT0_MB: C2RustUnnamed = 41;
pub const _NL_CTYPE_INDIGITS9_WC: C2RustUnnamed = 40;
pub const _NL_CTYPE_INDIGITS8_WC: C2RustUnnamed = 39;
pub const _NL_CTYPE_INDIGITS7_WC: C2RustUnnamed = 38;
pub const _NL_CTYPE_INDIGITS6_WC: C2RustUnnamed = 37;
pub const _NL_CTYPE_INDIGITS5_WC: C2RustUnnamed = 36;
pub const _NL_CTYPE_INDIGITS4_WC: C2RustUnnamed = 35;
pub const _NL_CTYPE_INDIGITS3_WC: C2RustUnnamed = 34;
pub const _NL_CTYPE_INDIGITS2_WC: C2RustUnnamed = 33;
pub const _NL_CTYPE_INDIGITS1_WC: C2RustUnnamed = 32;
pub const _NL_CTYPE_INDIGITS0_WC: C2RustUnnamed = 31;
pub const _NL_CTYPE_INDIGITS_WC_LEN: C2RustUnnamed = 30;
pub const _NL_CTYPE_INDIGITS9_MB: C2RustUnnamed = 29;
pub const _NL_CTYPE_INDIGITS8_MB: C2RustUnnamed = 28;
pub const _NL_CTYPE_INDIGITS7_MB: C2RustUnnamed = 27;
pub const _NL_CTYPE_INDIGITS6_MB: C2RustUnnamed = 26;
pub const _NL_CTYPE_INDIGITS5_MB: C2RustUnnamed = 25;
pub const _NL_CTYPE_INDIGITS4_MB: C2RustUnnamed = 24;
pub const _NL_CTYPE_INDIGITS3_MB: C2RustUnnamed = 23;
pub const _NL_CTYPE_INDIGITS2_MB: C2RustUnnamed = 22;
pub const _NL_CTYPE_INDIGITS1_MB: C2RustUnnamed = 21;
pub const _NL_CTYPE_INDIGITS0_MB: C2RustUnnamed = 20;
pub const _NL_CTYPE_INDIGITS_MB_LEN: C2RustUnnamed = 19;
pub const _NL_CTYPE_MAP_OFFSET: C2RustUnnamed = 18;
pub const _NL_CTYPE_CLASS_OFFSET: C2RustUnnamed = 17;
pub const _NL_CTYPE_TOLOWER32: C2RustUnnamed = 16;
pub const _NL_CTYPE_TOUPPER32: C2RustUnnamed = 15;
pub const CODESET: C2RustUnnamed = 14;
pub const _NL_CTYPE_CODESET_NAME: C2RustUnnamed = 14;
pub const _NL_CTYPE_MB_CUR_MAX: C2RustUnnamed = 13;
pub const _NL_CTYPE_WIDTH: C2RustUnnamed = 12;
pub const _NL_CTYPE_MAP_NAMES: C2RustUnnamed = 11;
pub const _NL_CTYPE_CLASS_NAMES: C2RustUnnamed = 10;
pub const _NL_CTYPE_GAP6: C2RustUnnamed = 9;
pub const _NL_CTYPE_GAP5: C2RustUnnamed = 8;
pub const _NL_CTYPE_GAP4: C2RustUnnamed = 7;
pub const _NL_CTYPE_GAP3: C2RustUnnamed = 6;
pub const _NL_CTYPE_CLASS32: C2RustUnnamed = 5;
pub const _NL_CTYPE_GAP2: C2RustUnnamed = 4;
pub const _NL_CTYPE_TOLOWER: C2RustUnnamed = 3;
pub const _NL_CTYPE_GAP1: C2RustUnnamed = 2;
pub const _NL_CTYPE_TOUPPER: C2RustUnnamed = 1;
pub const _NL_CTYPE_CLASS: C2RustUnnamed = 0;
pub const _NL_NUM_LC_COLLATE: C2RustUnnamed = 196627;
pub const _NL_COLLATE_CODESET: C2RustUnnamed = 196626;
pub const _NL_COLLATE_COLLSEQWC: C2RustUnnamed = 196625;
pub const _NL_COLLATE_COLLSEQMB: C2RustUnnamed = 196624;
pub const _NL_COLLATE_SYMB_EXTRAMB: C2RustUnnamed = 196623;
pub const _NL_COLLATE_SYMB_TABLEMB: C2RustUnnamed = 196622;
pub const _NL_COLLATE_SYMB_HASH_SIZEMB: C2RustUnnamed = 196621;
pub const _NL_COLLATE_INDIRECTWC: C2RustUnnamed = 196620;
pub const _NL_COLLATE_EXTRAWC: C2RustUnnamed = 196619;
pub const _NL_COLLATE_WEIGHTWC: C2RustUnnamed = 196618;
pub const _NL_COLLATE_TABLEWC: C2RustUnnamed = 196617;
pub const _NL_COLLATE_GAP3: C2RustUnnamed = 196616;
pub const _NL_COLLATE_GAP2: C2RustUnnamed = 196615;
pub const _NL_COLLATE_GAP1: C2RustUnnamed = 196614;
pub const _NL_COLLATE_INDIRECTMB: C2RustUnnamed = 196613;
pub const _NL_COLLATE_EXTRAMB: C2RustUnnamed = 196612;
pub const _NL_COLLATE_WEIGHTMB: C2RustUnnamed = 196611;
pub const _NL_COLLATE_TABLEMB: C2RustUnnamed = 196610;
pub const _NL_COLLATE_RULESETS: C2RustUnnamed = 196609;
pub const _NL_COLLATE_NRULES: C2RustUnnamed = 196608;
pub const _NL_NUM_LC_TIME: C2RustUnnamed = 131231;
pub const _NL_WABALTMON_12: C2RustUnnamed = 131230;
pub const _NL_WABALTMON_11: C2RustUnnamed = 131229;
pub const _NL_WABALTMON_10: C2RustUnnamed = 131228;
pub const _NL_WABALTMON_9: C2RustUnnamed = 131227;
pub const _NL_WABALTMON_8: C2RustUnnamed = 131226;
pub const _NL_WABALTMON_7: C2RustUnnamed = 131225;
pub const _NL_WABALTMON_6: C2RustUnnamed = 131224;
pub const _NL_WABALTMON_5: C2RustUnnamed = 131223;
pub const _NL_WABALTMON_4: C2RustUnnamed = 131222;
pub const _NL_WABALTMON_3: C2RustUnnamed = 131221;
pub const _NL_WABALTMON_2: C2RustUnnamed = 131220;
pub const _NL_WABALTMON_1: C2RustUnnamed = 131219;
pub const _NL_ABALTMON_12: C2RustUnnamed = 131218;
pub const _NL_ABALTMON_11: C2RustUnnamed = 131217;
pub const _NL_ABALTMON_10: C2RustUnnamed = 131216;
pub const _NL_ABALTMON_9: C2RustUnnamed = 131215;
pub const _NL_ABALTMON_8: C2RustUnnamed = 131214;
pub const _NL_ABALTMON_7: C2RustUnnamed = 131213;
pub const _NL_ABALTMON_6: C2RustUnnamed = 131212;
pub const _NL_ABALTMON_5: C2RustUnnamed = 131211;
pub const _NL_ABALTMON_4: C2RustUnnamed = 131210;
pub const _NL_ABALTMON_3: C2RustUnnamed = 131209;
pub const _NL_ABALTMON_2: C2RustUnnamed = 131208;
pub const _NL_ABALTMON_1: C2RustUnnamed = 131207;
pub const _NL_WALTMON_12: C2RustUnnamed = 131206;
pub const _NL_WALTMON_11: C2RustUnnamed = 131205;
pub const _NL_WALTMON_10: C2RustUnnamed = 131204;
pub const _NL_WALTMON_9: C2RustUnnamed = 131203;
pub const _NL_WALTMON_8: C2RustUnnamed = 131202;
pub const _NL_WALTMON_7: C2RustUnnamed = 131201;
pub const _NL_WALTMON_6: C2RustUnnamed = 131200;
pub const _NL_WALTMON_5: C2RustUnnamed = 131199;
pub const _NL_WALTMON_4: C2RustUnnamed = 131198;
pub const _NL_WALTMON_3: C2RustUnnamed = 131197;
pub const _NL_WALTMON_2: C2RustUnnamed = 131196;
pub const _NL_WALTMON_1: C2RustUnnamed = 131195;
pub const __ALTMON_12: C2RustUnnamed = 131194;
pub const __ALTMON_11: C2RustUnnamed = 131193;
pub const __ALTMON_10: C2RustUnnamed = 131192;
pub const __ALTMON_9: C2RustUnnamed = 131191;
pub const __ALTMON_8: C2RustUnnamed = 131190;
pub const __ALTMON_7: C2RustUnnamed = 131189;
pub const __ALTMON_6: C2RustUnnamed = 131188;
pub const __ALTMON_5: C2RustUnnamed = 131187;
pub const __ALTMON_4: C2RustUnnamed = 131186;
pub const __ALTMON_3: C2RustUnnamed = 131185;
pub const __ALTMON_2: C2RustUnnamed = 131184;
pub const __ALTMON_1: C2RustUnnamed = 131183;
pub const _NL_TIME_CODESET: C2RustUnnamed = 131182;
pub const _NL_W_DATE_FMT: C2RustUnnamed = 131181;
pub const _DATE_FMT: C2RustUnnamed = 131180;
pub const _NL_TIME_TIMEZONE: C2RustUnnamed = 131179;
pub const _NL_TIME_CAL_DIRECTION: C2RustUnnamed = 131178;
pub const _NL_TIME_FIRST_WORKDAY: C2RustUnnamed = 131177;
pub const _NL_TIME_FIRST_WEEKDAY: C2RustUnnamed = 131176;
pub const _NL_TIME_WEEK_1STWEEK: C2RustUnnamed = 131175;
pub const _NL_TIME_WEEK_1STDAY: C2RustUnnamed = 131174;
pub const _NL_TIME_WEEK_NDAYS: C2RustUnnamed = 131173;
pub const _NL_WERA_T_FMT: C2RustUnnamed = 131172;
pub const _NL_WERA_D_T_FMT: C2RustUnnamed = 131171;
pub const _NL_WALT_DIGITS: C2RustUnnamed = 131170;
pub const _NL_WERA_D_FMT: C2RustUnnamed = 131169;
pub const _NL_WERA_YEAR: C2RustUnnamed = 131168;
pub const _NL_WT_FMT_AMPM: C2RustUnnamed = 131167;
pub const _NL_WT_FMT: C2RustUnnamed = 131166;
pub const _NL_WD_FMT: C2RustUnnamed = 131165;
pub const _NL_WD_T_FMT: C2RustUnnamed = 131164;
pub const _NL_WPM_STR: C2RustUnnamed = 131163;
pub const _NL_WAM_STR: C2RustUnnamed = 131162;
pub const _NL_WMON_12: C2RustUnnamed = 131161;
pub const _NL_WMON_11: C2RustUnnamed = 131160;
pub const _NL_WMON_10: C2RustUnnamed = 131159;
pub const _NL_WMON_9: C2RustUnnamed = 131158;
pub const _NL_WMON_8: C2RustUnnamed = 131157;
pub const _NL_WMON_7: C2RustUnnamed = 131156;
pub const _NL_WMON_6: C2RustUnnamed = 131155;
pub const _NL_WMON_5: C2RustUnnamed = 131154;
pub const _NL_WMON_4: C2RustUnnamed = 131153;
pub const _NL_WMON_3: C2RustUnnamed = 131152;
pub const _NL_WMON_2: C2RustUnnamed = 131151;
pub const _NL_WMON_1: C2RustUnnamed = 131150;
pub const _NL_WABMON_12: C2RustUnnamed = 131149;
pub const _NL_WABMON_11: C2RustUnnamed = 131148;
pub const _NL_WABMON_10: C2RustUnnamed = 131147;
pub const _NL_WABMON_9: C2RustUnnamed = 131146;
pub const _NL_WABMON_8: C2RustUnnamed = 131145;
pub const _NL_WABMON_7: C2RustUnnamed = 131144;
pub const _NL_WABMON_6: C2RustUnnamed = 131143;
pub const _NL_WABMON_5: C2RustUnnamed = 131142;
pub const _NL_WABMON_4: C2RustUnnamed = 131141;
pub const _NL_WABMON_3: C2RustUnnamed = 131140;
pub const _NL_WABMON_2: C2RustUnnamed = 131139;
pub const _NL_WABMON_1: C2RustUnnamed = 131138;
pub const _NL_WDAY_7: C2RustUnnamed = 131137;
pub const _NL_WDAY_6: C2RustUnnamed = 131136;
pub const _NL_WDAY_5: C2RustUnnamed = 131135;
pub const _NL_WDAY_4: C2RustUnnamed = 131134;
pub const _NL_WDAY_3: C2RustUnnamed = 131133;
pub const _NL_WDAY_2: C2RustUnnamed = 131132;
pub const _NL_WDAY_1: C2RustUnnamed = 131131;
pub const _NL_WABDAY_7: C2RustUnnamed = 131130;
pub const _NL_WABDAY_6: C2RustUnnamed = 131129;
pub const _NL_WABDAY_5: C2RustUnnamed = 131128;
pub const _NL_WABDAY_4: C2RustUnnamed = 131127;
pub const _NL_WABDAY_3: C2RustUnnamed = 131126;
pub const _NL_WABDAY_2: C2RustUnnamed = 131125;
pub const _NL_WABDAY_1: C2RustUnnamed = 131124;
pub const _NL_TIME_ERA_ENTRIES: C2RustUnnamed = 131123;
pub const _NL_TIME_ERA_NUM_ENTRIES: C2RustUnnamed = 131122;
pub const __ERA_YEAR: C2RustUnnamed = 131117;

pub const __S_IFMT: ::core::ffi::c_int = 0o170000 as ::core::ffi::c_int;

pub const EEXIST: ::core::ffi::c_int = 17 as ::core::ffi::c_int;
pub const __LC_CTYPE: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const __LC_TIME: ::core::ffi::c_int = 2 as ::core::ffi::c_int;

pub const F_GETFL: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub const F_SETFL: ::core::ffi::c_int = 4 as ::core::ffi::c_int;

pub const LC_CTYPE: ::core::ffi::c_int = __LC_CTYPE;
pub const LC_TIME: ::core::ffi::c_int = __LC_TIME;

pub const CLOCK_MONOTONIC: ::core::ffi::c_int = 1 as ::core::ffi::c_int;

pub const TMUX_SOCK_PERM: ::core::ffi::c_int = 7 as ::core::ffi::c_int;
// Sole owners; the public pointers below only observe these stable allocations.
static mut global_options_owner: Option<Box<options>> = None;
static mut global_s_options_owner: Option<Box<options>> = None;
static mut global_w_options_owner: Option<Box<options>> = None;

pub(crate) unsafe fn free_global_options() {
    if let Some(owner) = global_options_owner.take() {
        drop(owner);
        global_options = std::ptr::null_mut();
    }
    if let Some(owner) = global_s_options_owner.take() {
        drop(owner);
        global_s_options = std::ptr::null_mut();
    }
    if let Some(owner) = global_w_options_owner.take() {
        drop(owner);
        global_w_options = std::ptr::null_mut();
    }
}

pub static mut global_options: *mut options = ::core::ptr::null::<options>() as *mut options;
pub static mut global_s_options: *mut options = ::core::ptr::null::<options>() as *mut options;
pub static mut global_w_options: *mut options = ::core::ptr::null::<options>() as *mut options;
pub static mut global_environ: Option<Box<environ>> = None;
pub static mut start_time: SystemTime = UNIX_EPOCH;
pub static mut socket_path: Option<CString> = None;
pub static mut ptm_fd: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
pub static mut shell_command: Option<CString> = None;
/// The socket path chosen at startup (or by systemd activation).
pub unsafe fn socket_path_cstr() -> &'static CStr {
    socket_path
        .as_deref()
        .expect("socket path is set")
}
unsafe fn usage(mut status: ::core::ffi::c_int) -> ! {
    fprintf(
        if status != 0 { stderr } else { stdout },
        c"usage: %s [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n            [-S socket-path] [-T features] [command [flags]]\n".as_ptr(),
        getprogname().as_ptr(),
    );
    exit(status);
}
unsafe fn getshell() -> CString {
    let shell = getenv(c"SHELL".as_ptr());
    if checkshell(nullable_cstr(shell)) != 0 {
        return CStr::from_ptr(shell).to_owned();
    }
    let pw = getpwuid(getuid());
    if !pw.is_null() && checkshell(nullable_cstr((*pw).pw_shell)) != 0 {
        return CStr::from_ptr((*pw).pw_shell).to_owned();
    }
    c"/bin/sh".to_owned()
}
pub unsafe fn checkshell(shell: Option<&CStr>) -> ::core::ffi::c_int {
    let shell: *const ::core::ffi::c_char = shell.map_or(std::ptr::null(), CStr::as_ptr);
    if shell.is_null() || *shell as ::core::ffi::c_int != '/' as i32 {
        return 0 as ::core::ffi::c_int;
    }
    if areshell(CStr::from_ptr(shell)) != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if access(shell, X_OK) != 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
unsafe fn areshell(shell: &CStr) -> ::core::ffi::c_int {
    let mut progname: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut ptr: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    ptr = strrchr(shell.as_ptr(), '/' as i32);
    if !ptr.is_null() {
        ptr = ptr.offset(1);
    } else {
        ptr = shell.as_ptr();
    }
    progname = getprogname().as_ptr();
    if *progname as ::core::ffi::c_int == '-' as i32 {
        progname = progname.offset(1);
    }
    if strcmp(ptr, progname) == 0 as ::core::ffi::c_int {
        return 1 as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}
unsafe fn expand_path(path: &CStr, home: Option<&CStr>) -> Option<CString> {
    let mut value: Option<&environ_entry> = None;
    let path_bytes = path.to_bytes();
    if path_bytes.starts_with(b"~/") {
        let mut expanded = home?.to_bytes().to_vec();
        expanded.extend_from_slice(&path_bytes[1..]);
        return Some(CString::new(expanded).expect("C strings contain no interior NUL"));
    }
    if path_bytes.first() == Some(&b'$') {
        let slash = path_bytes.iter().position(|byte| *byte == b'/');
        let name_end = slash.unwrap_or(path_bytes.len());
        let name =
            CString::new(&path_bytes[1..name_end]).expect("variable name comes from a C string");
        value = environ_find(global_environ.as_deref().expect("environment"), &name);
        value?;
        // On glibc, the previous `%s` rendered a cleared environment value
        // as `(null)`. Keep that behavior if this entry has no value.
        let mut expanded = if value.unwrap().value.is_none() {
            b"(null)".to_vec()
        } else {
            (value.unwrap().value)
                .as_deref()
                .expect("string is present")
                .to_bytes()
                .to_vec()
        };
        if let Some(slash) = slash {
            expanded.extend_from_slice(&path_bytes[slash..]);
        }
        return Some(CString::new(expanded).expect("C strings contain no interior NUL"));
    }
    Some(path.to_owned())
}
unsafe fn expand_paths(s: &CStr, no_realpath: bool) -> Vec<CString> {
    let home = find_home_cstr();
    let mut next: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut tmp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut resolved: [::core::ffi::c_char; 4096] = [0; 4096];
    let mut paths = Vec::new();
    // strsep rewrites separators in place; keep its borrowed token pointers
    // backed by one stable, NUL-terminated allocation for the entire loop.
    let mut copy = s.to_bytes_with_nul().to_vec();
    tmp = copy.as_mut_ptr().cast();
    loop {
        next = strsep(&raw mut tmp, c":".as_ptr());
        if next.is_null() {
            break;
        }
        if let Some(expanded) = expand_path(CStr::from_ptr(next), home) {
            let path = if no_realpath {
                Some(expanded)
            } else {
                let path = if realpath(
                    expanded.as_ptr(),
                    &raw mut resolved as *mut ::core::ffi::c_char,
                )
                .is_null()
                {
                    log_debug(format_args!(
                        "{}: realpath(\"{}\") failed: {}",
                        "expand_paths",
                        log_cstr(&expanded),
                        log_cstr(CStr::from_ptr(strerror(*__errno_location())))
                    ));
                    None
                } else {
                    Some(CStr::from_ptr(resolved.as_ptr()).to_owned())
                };
                drop(expanded);
                path
            };
            if let Some(path) = path {
                if paths.iter().any(|existing| existing == &path) {
                    log_debug(format_args!(
                        "{}: duplicate path: {}",
                        "expand_paths",
                        log_cstr(&path)
                    ));
                } else {
                    paths.push(path);
                }
            }
        } else {
            log_debug(format_args!(
                "{}: invalid path: {}",
                "expand_paths",
                log_cstr(CStr::from_ptr(next))
            ));
        }
    }
    paths
}
fn make_label_cause(parts: &[&[u8]]) -> CString {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(part);
    }
    CString::new(bytes).expect("socket label cause contains no NUL")
}

unsafe fn make_label(label: Option<&CStr>) -> Result<CString, CString> {
    let mut sb: stat = stat {
        st_dev: 0,
        st_ino: 0,
        st_nlink: 0,
        st_mode: 0,
        st_uid: 0,
        st_gid: 0,
        __pad0: 0,
        st_rdev: 0,
        st_size: 0,
        st_blksize: 0,
        st_blocks: 0,
        st_atim: timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        st_mtim: timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        st_ctim: timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        __glibc_reserved: [0; 3],
    };
    let mut uid: uid_t = 0;
    uid = getuid() as uid_t;
    let paths = expand_paths(c"$TMUX_TMPDIR:/tmp/", false);
    if paths.is_empty() {
        return Err(c"no suitable socket path".to_owned());
    }
    let mut base_bytes = paths[0].to_bytes().to_vec();
    base_bytes.extend_from_slice(b"/tmux-");
    base_bytes.extend_from_slice((uid as ::core::ffi::c_long).to_string().as_bytes());
    let base = CString::new(base_bytes).expect("socket base contains no NUL");
    if mkdir(base.as_ptr(), S_IRWXU as __mode_t) != 0 as ::core::ffi::c_int
        && *__errno_location() != EEXIST
    {
        let error = CStr::from_ptr(strerror(*__errno_location()));
        Err(make_label_cause(&[
            b"couldn't create directory ",
            base.to_bytes(),
            b" (",
            error.to_bytes(),
            b")",
        ]))
    } else if lstat(base.as_ptr(), &raw mut sb) != 0 as ::core::ffi::c_int {
        let error = CStr::from_ptr(strerror(*__errno_location()));
        Err(make_label_cause(&[
            b"couldn't read directory ",
            base.to_bytes(),
            b" (",
            error.to_bytes(),
            b")",
        ]))
    } else if !(sb.st_mode & __S_IFMT as __mode_t == 0o40000 as __mode_t) {
        Err(make_label_cause(&[base.to_bytes(), b" is not a directory"]))
    } else if sb.st_uid != uid || sb.st_mode & TMUX_SOCK_PERM as __mode_t != 0 as __mode_t {
        Err(make_label_cause(&[
            b"directory ",
            base.to_bytes(),
            b" has unsafe permissions",
        ]))
    } else {
        let mut path = base.into_bytes();
        path.push(b'/');
        path.extend_from_slice(label.unwrap_or(c"default").to_bytes());
        Ok(CString::new(path).expect("socket label path contains no NUL"))
    }
}
pub(crate) unsafe fn shell_argv0_cstring(shell: &CStr, is_login: bool) -> CString {
    let shell = shell.to_bytes();
    let name = match shell.iter().rposition(|&byte| byte == b'/') {
        Some(slash) if slash + 1 < shell.len() => &shell[slash + 1..],
        _ => shell,
    };
    let mut argv0 = Vec::with_capacity(name.len() + usize::from(is_login));
    if is_login {
        argv0.push(b'-');
    }
    argv0.extend_from_slice(name);
    CString::new(argv0).expect("shell path contains no NUL")
}
pub unsafe fn setblocking(fd: ::core::ffi::c_int, state: ::core::ffi::c_int) {
    if fd >= 0 {
        let _ = hmux_rt::unix::set_nonblocking(std::os::fd::BorrowedFd::borrow_raw(fd), state == 0);
    }
}
pub unsafe fn get_timer() -> uint64_t {
    let mut ts: timespec = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if clock_gettime(CLOCK_MONOTONIC, &raw mut ts) != 0 as ::core::ffi::c_int {
        clock_gettime(CLOCK_REALTIME, &raw mut ts);
    }
    (ts.tv_sec as ::core::ffi::c_ulonglong)
        .wrapping_mul(1000 as ::core::ffi::c_ulonglong)
        .wrapping_add(
            (ts.tv_nsec as ::core::ffi::c_ulonglong)
                .wrapping_div(1000000 as ::core::ffi::c_ulonglong),
        ) as uint64_t
}
/// Escape a validated name once before moving it into its Rust owner.
pub fn clean_name_cstring(name: &CStr, untrusted: ::core::ffi::c_int) -> Option<CString> {
    if !utf8_isvalid(name) {
        return None;
    }
    let mut copy = name.to_bytes_with_nul().to_vec();
    if untrusted != 0 {
        for i in 0..copy.len() - 1 {
            if copy[i] == b'#' && copy[i + 1] == b'(' {
                copy[i] = b'_';
            }
        }
    }
    Some(utf8_stravis_cstring(
        CStr::from_bytes_with_nul(&copy).expect("copied name has one trailing NUL"),
        VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL,
    ))
}
pub fn check_name(name: &CStr) -> bool {
    utf8_isvalid(name)
}
pub fn sig2name(signo: ::core::ffi::c_int) -> CString {
    CString::new(signo.to_string()).expect("decimal number has no NUL")
}
pub unsafe fn find_cwd() -> Option<CString> {
    let mut resolved1: [::core::ffi::c_char; 4096] = [0; 4096];
    let mut resolved2: [::core::ffi::c_char; 4096] = [0; 4096];
    let mut buffer: [::core::ffi::c_char; 4096] = [0; 4096];
    if getcwd(buffer.as_mut_ptr(), buffer.len() as size_t).is_null() {
        return None;
    }
    let cwd = CStr::from_ptr(buffer.as_ptr()).to_owned();
    let pwd = getenv(c"PWD".as_ptr());
    if pwd.is_null()
        || *pwd as ::core::ffi::c_int == '\0' as i32
        || realpath(pwd, resolved1.as_mut_ptr()).is_null()
        || realpath(cwd.as_ptr(), resolved2.as_mut_ptr()).is_null()
        || strcmp(resolved1.as_ptr(), resolved2.as_ptr()) != 0 as ::core::ffi::c_int
    {
        return Some(cwd);
    }
    Some(CStr::from_ptr(pwd).to_owned())
}
/// Return the cached home directory as a borrowed C string. A missing result
/// is retried on the next call, matching the old null-sentinel cache behavior.
pub(crate) unsafe fn find_home_cstr() -> Option<&'static CStr> {
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    static HOME: OnceLock<CString> = OnceLock::new();
    if let Some(home) = HOME.get() {
        return Some(home.as_c_str());
    }
    let mut home = getenv(c"HOME".as_ptr());
    if home.is_null() || *home as ::core::ffi::c_int == '\0' as i32 {
        pw = getpwuid(getuid());
        if !pw.is_null() {
            home = (*pw).pw_dir;
        } else {
            return None;
        }
    }
    let owned = CStr::from_ptr(home).to_owned();
    let _ = HOME.set(owned);
    HOME.get().map(CString::as_c_str)
}

pub fn getversion() -> &'static CStr {
    c"3.8"
}
unsafe fn main_0(args: &[CString]) -> ::core::ffi::c_int {
    let mut argc = ::core::ffi::c_int::try_from(args.len()).expect("argv length exceeds c_int");
    let mut argv_view: Vec<_> = args.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
    argv_view.push(::core::ptr::null_mut());
    let mut argv = argv_view.as_mut_ptr();
    let mut path: Option<CString> = None;
    // BSDoptarg points into argv, which main keeps alive through main_0.
    let mut label: Option<&CStr> = None;
    let mut var: *mut *mut ::core::ffi::c_char =
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut opt: ::core::ffi::c_int = 0;
    let mut keys: ::core::ffi::c_int = 0;
    let mut feat: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut fflag: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut flags: uint64_t = 0 as uint64_t;
    if setlocale(LC_CTYPE, c"en_US.UTF-8".as_ptr()).is_null()
        && setlocale(LC_CTYPE, c"C.UTF-8".as_ptr()).is_null()
    {
        if setlocale(LC_CTYPE, c"".as_ptr()).is_null() {
            errx(
                1 as ::core::ffi::c_int,
                c"invalid LC_ALL, LC_CTYPE or LANG".as_ptr(),
            );
        }
        s = nl_langinfo(CODESET as ::core::ffi::c_int as nl_item);
        if strcasecmp(s, c"UTF-8".as_ptr()) != 0 as ::core::ffi::c_int
            && strcasecmp(s, c"UTF8".as_ptr()) != 0 as ::core::ffi::c_int
        {
            errx(
                1 as ::core::ffi::c_int,
                c"need UTF-8 locale (LC_CTYPE) but have %s".as_ptr(),
                s,
            );
        }
    }
    setlocale(LC_TIME, c"".as_ptr());
    tzset();
    if args.first().and_then(|arg| arg.as_bytes().first()) == Some(&b'-') {
        flags = CLIENT_LOGIN as uint64_t;
    }
    global_environ = Some(environ_create());
    var = environ;
    while !(*var).is_null() {
        environ_put(
            global_environ.as_deref_mut().expect("environment"),
            CStr::from_ptr(*var),
            0 as ::core::ffi::c_int,
        );
        var = var.offset(1);
    }
    if let Some(cwd) = find_cwd() {
        environ_set(
            global_environ.as_deref_mut().expect("environment"),
            c"PWD",
            0 as ::core::ffi::c_int,
            |out| out.write_all(cwd.as_bytes()),
        );
    }
    let mut config_paths = expand_paths(TMUX_CONF, true);
    loop {
        opt = BSDgetopt(argc, argv, c"2c:CDdf:hlL:NqS:T:uUvV".as_ptr());
        if !(opt != -(1 as ::core::ffi::c_int)) {
            break;
        }
        match opt {
            50 => {
                tty_parse_features(
                    c"256",
                    c":,",
                    &raw mut feat,
                    ::core::ptr::null_mut::<::core::ffi::c_int>(),
                );
            }
            99 => {
                shell_command = Some(CStr::from_ptr(BSDoptarg).to_owned());
            }
            68 => {
                flags |= CLIENT_NOFORK as uint64_t;
            }
            67 => {
                if flags & CLIENT_CONTROL as uint64_t != 0 {
                    flags |= CLIENT_CONTROLCONTROL as uint64_t;
                } else {
                    flags |= CLIENT_CONTROL as uint64_t;
                }
            }
            102 => {
                if fflag == 0 {
                    fflag = 1 as ::core::ffi::c_int;
                    config_paths.clear();
                }
                config_paths.push(CStr::from_ptr(BSDoptarg).to_owned());
                cfg_quiet = 0 as ::core::ffi::c_int;
            }
            104 => {
                usage(0 as ::core::ffi::c_int);
            }
            86 => {
                printf(c"tmux %s\n".as_ptr(), getversion().as_ptr());
                exit(0 as ::core::ffi::c_int);
            }
            108 => {
                flags |= CLIENT_LOGIN as uint64_t;
            }
            76 => {
                label = Some(CStr::from_ptr(BSDoptarg));
            }
            78 => {
                flags |= CLIENT_NOSTARTSERVER as uint64_t;
            }
            113 => {}
            83 => {
                path = Some(CStr::from_ptr(BSDoptarg).to_owned());
            }
            84 => {
                tty_parse_features(
                    CStr::from_ptr(BSDoptarg),
                    c":,",
                    &raw mut feat,
                    ::core::ptr::null_mut::<::core::ffi::c_int>(),
                );
            }
            117 => {
                flags |= CLIENT_UTF8 as uint64_t;
            }
            118 => {
                log_add_level();
            }
            _ => {
                usage(1 as ::core::ffi::c_int);
            }
        }
    }
    cfg_set_files(config_paths);
    argc -= BSDoptind;
    argv = argv.offset(BSDoptind as isize);
    if shell_command.is_some() && argc != 0 as ::core::ffi::c_int {
        usage(1 as ::core::ffi::c_int);
    }
    if flags & CLIENT_NOFORK as uint64_t != 0 && argc != 0 as ::core::ffi::c_int {
        usage(1 as ::core::ffi::c_int);
    }
    ptm_fd = getptmfd();
    if ptm_fd == -(1 as ::core::ffi::c_int) {
        err(1 as ::core::ffi::c_int, c"getptmfd".as_ptr());
    }
    if 0 as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
        err(1 as ::core::ffi::c_int, c"pledge".as_ptr());
    }
    if !getenv(c"TMUX".as_ptr()).is_null() {
        flags |= CLIENT_UTF8 as uint64_t;
    } else {
        s = getenv(c"LC_ALL".as_ptr());
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = getenv(c"LC_CTYPE".as_ptr());
        }
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = getenv(c"LANG".as_ptr());
        }
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = c"".as_ptr();
        }
        if !strcasestr(s, c"UTF-8".as_ptr()).is_null() || !strcasestr(s, c"UTF8".as_ptr()).is_null()
        {
            flags |= CLIENT_UTF8 as uint64_t;
        }
    }
    global_options_owner = Some(options_create(None));
    global_options = global_options_owner.as_deref_mut().unwrap();
    global_s_options_owner = Some(options_create(None));
    global_s_options = global_s_options_owner.as_deref_mut().unwrap();
    global_w_options_owner = Some(options_create(None));
    global_w_options = global_w_options_owner.as_deref_mut().unwrap();
    for oe in options_table
        .iter()
        .take_while(|entry| entry.name.is_some())
    {
        if oe.scope & OPTIONS_TABLE_SERVER != 0 {
            options_default(global_options, oe);
        }
        if oe.scope & OPTIONS_TABLE_SESSION != 0 {
            options_default(global_s_options, oe);
        }
        if oe.scope & OPTIONS_TABLE_WINDOW != 0 {
            options_default(global_w_options, oe);
        }
    }
    options_set_string(
        global_s_options,
        c"default-shell",
        0 as ::core::ffi::c_int,
        |out| out.write_all(getshell().as_bytes()),
    );
    s = getenv(c"VISUAL".as_ptr());
    if !s.is_null() || {
        s = getenv(c"EDITOR".as_ptr());
        !s.is_null()
    } {
        options_set_string(global_options, c"editor", 0 as ::core::ffi::c_int, |out| {
            write_cstr(out, nullable_cstr(s))
        });
        if !strrchr(s, '/' as i32).is_null() {
            s = strrchr(s, '/' as i32).offset(1 as ::core::ffi::c_int as isize);
        }
        if !strstr(s, c"vi".as_ptr()).is_null() {
            keys = MODEKEY_VI;
        } else {
            keys = MODEKEY_EMACS;
        }
        options_set_number(
            global_s_options,
            c"status-keys",
            keys as ::core::ffi::c_longlong,
        );
        options_set_number(
            global_w_options,
            c"mode-keys",
            keys as ::core::ffi::c_longlong,
        );
    }
    if path.is_none() && label.is_none() {
        s = getenv(c"TMUX".as_ptr());
        if !s.is_null()
            && *s as ::core::ffi::c_int != '\0' as i32
            && *s as ::core::ffi::c_int != ',' as i32
        {
            let tmux = CStr::from_ptr(s).to_bytes();
            let end = tmux
                .iter()
                .position(|byte| *byte == b',')
                .unwrap_or(tmux.len());
            path = Some(CString::new(&tmux[..end]).expect("TMUX socket path contains no NUL"));
        }
    }
    if path.is_none() {
        path = Some(match make_label(label) {
            Ok(path) => path,
            Err(cause) => {
                fprintf(stderr, c"%s\n".as_ptr(), cause.as_ptr());
                drop(cause);
                exit(1 as ::core::ffi::c_int);
            }
        });
        flags |= CLIENT_DEFAULTSOCKET as uint64_t;
    }
    // Systemd activation may replace the global separately.
    socket_path = Some(path.expect("socket path was selected"));
    let command_argv: Vec<CString> = (0..argc)
        .map(|i| CStr::from_ptr(*argv.add(i as usize)).to_owned())
        .collect();
    exit(client_main(&command_argv, flags, feat));
}
pub const TMUX_CONF: &std::ffi::CStr =
    c"/etc/tmux.conf:~/.tmux.conf:$XDG_CONFIG_HOME/tmux/tmux.conf:~/.config/tmux/tmux.conf";
pub fn main() {
    let args_strings: Vec<CString> = ::std::env::args_os()
        .map(|arg| {
            CString::new(::std::os::unix::ffi::OsStrExt::as_bytes(arg.as_os_str()))
                .expect("Failed to convert argument into CString.")
        })
        .collect();
    unsafe { ::std::process::exit(main_0(&args_strings) as i32) }
}
