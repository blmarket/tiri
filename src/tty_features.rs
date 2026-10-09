use crate::src::ffi::libc::{strcasecmp, strcmp, strlen, strsep};
use crate::src::log::{log_cstr, log_debug};
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::tty::tty_term;
use crate::src::shared::tty::{
    TERM_256COLOURS, TERM_DECFRA, TERM_DECSLRM, TERM_RGBCOLOURS, TERM_SIXEL,
};
use crate::src::tty_term::{tty_term_apply, tty_term_has_name};
use std::ffi::{CStr, CString};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct tty_feature {
    pub name: &'static CStr,
    pub capabilities: Option<&'static [&'static CStr]>,
    pub flags: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_35 {
    pub name: &'static CStr,
    pub version: u_int,
    pub features: &'static CStr,
}
static tty_feature_title_capabilities: &[&CStr] = &[c"tsl=\\E]0;", c"fsl=\\a"];
static tty_feature_title: tty_feature = tty_feature {
    name: c"title",
    capabilities: Some(tty_feature_title_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_osc7_capabilities: &[&CStr] = &[c"Swd=\\E]7;", c"fsl=\\a"];
static tty_feature_osc7: tty_feature = tty_feature {
    name: c"osc7",
    capabilities: Some(tty_feature_osc7_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_mouse_capabilities: &[&CStr] = &[c"kmous=\\E[M"];
static tty_feature_mouse: tty_feature = tty_feature {
    name: c"mouse",
    capabilities: Some(tty_feature_mouse_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_clipboard_capabilities: &[&CStr] = &[c"Ms=\\E]52;%p1%s;%p2%s\\a"];
static tty_feature_clipboard: tty_feature = tty_feature {
    name: c"clipboard",
    capabilities: Some(tty_feature_clipboard_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_hyperlinks_capabilities: &[&CStr] =
    &[c"Hls=\\E]8;%?%p1%l%tid=%p1%s%;;%p2%s\\E\\\\"];
static tty_feature_hyperlinks: tty_feature = tty_feature {
    name: c"hyperlinks",
    capabilities: Some(tty_feature_hyperlinks_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_rgb_capabilities: &[&CStr] = &[
    c"AX",
    c"setrgbf=\\E[38;2;%p1%d;%p2%d;%p3%dm",
    c"setrgbb=\\E[48;2;%p1%d;%p2%d;%p3%dm",
    c"setab=\\E[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m",
    c"setaf=\\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
];
static tty_feature_rgb: tty_feature = tty_feature {
    name: c"RGB",
    capabilities: Some(tty_feature_rgb_capabilities),
    flags: TERM_256COLOURS | TERM_RGBCOLOURS,
};
static tty_feature_256_capabilities: &[&CStr] = &[
    c"AX",
    c"setab=\\E[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m",
    c"setaf=\\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m",
];
static tty_feature_256: tty_feature = tty_feature {
    name: c"256",
    capabilities: Some(tty_feature_256_capabilities),
    flags: TERM_256COLOURS,
};
static tty_feature_overline_capabilities: &[&CStr] = &[c"Smol=\\E[53m"];
static tty_feature_overline: tty_feature = tty_feature {
    name: c"overline",
    capabilities: Some(tty_feature_overline_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_usstyle_capabilities: &[&CStr] = &[
    c"Smulx=\\E[4::%p1%dm",
    c"Setulc=\\E[58::2::%p1%{65536}%/%d::%p1%{256}%/%{255}%&%d::%p1%{255}%&%d%;m",
    c"Setulc1=\\E[58::5::%p1%dm",
    c"ol=\\E[59m",
];
static tty_feature_usstyle: tty_feature = tty_feature {
    name: c"usstyle",
    capabilities: Some(tty_feature_usstyle_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_bpaste_capabilities: &[&CStr] = &[c"Enbp=\\E[?2004h", c"Dsbp=\\E[?2004l"];
static tty_feature_bpaste: tty_feature = tty_feature {
    name: c"bpaste",
    capabilities: Some(tty_feature_bpaste_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_focus_capabilities: &[&CStr] = &[c"Enfcs=\\E[?1004h", c"Dsfcs=\\E[?1004l"];
static tty_feature_focus: tty_feature = tty_feature {
    name: c"focus",
    capabilities: Some(tty_feature_focus_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_appesc_capabilities: &[&CStr] = &[c"Enesc=\\E[?7727h", c"Dsesc=\\E[?7727l"];
static tty_feature_appesc: tty_feature = tty_feature {
    name: c"appesc",
    capabilities: Some(tty_feature_appesc_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_cstyle_capabilities: &[&CStr] = &[c"Ss=\\E[%p1%d q", c"Se=\\E[2 q"];
static tty_feature_cstyle: tty_feature = tty_feature {
    name: c"cstyle",
    capabilities: Some(tty_feature_cstyle_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_ccolour_capabilities: &[&CStr] = &[c"Cs=\\E]12;%p1%s\\a", c"Cr=\\E]112\\a"];
static tty_feature_ccolour: tty_feature = tty_feature {
    name: c"ccolour",
    capabilities: Some(tty_feature_ccolour_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_strikethrough_capabilities: &[&CStr] = &[c"smxx=\\E[9m"];
static tty_feature_strikethrough: tty_feature = tty_feature {
    name: c"strikethrough",
    capabilities: Some(tty_feature_strikethrough_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_sync_capabilities: &[&CStr] = &[c"Sync=\\E[?2026%?%p1%{1}%-%tl%eh%;"];
static tty_feature_sync: tty_feature = tty_feature {
    name: c"sync",
    capabilities: Some(tty_feature_sync_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_extkeys_capabilities: &[&CStr] = &[c"Eneks=\\E[>4;2m", c"Dseks=\\E[>4m"];
static tty_feature_extkeys: tty_feature = tty_feature {
    name: c"extkeys",
    capabilities: Some(tty_feature_extkeys_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_margins_capabilities: &[&CStr] = &[
    c"Enmg=\\E[?69h",
    c"Dsmg=\\E[?69l",
    c"Clmg=\\E[s",
    c"Cmg=\\E[%i%p1%d;%p2%ds",
];
static tty_feature_margins: tty_feature = tty_feature {
    name: c"margins",
    capabilities: Some(tty_feature_margins_capabilities),
    flags: TERM_DECSLRM,
};
static tty_feature_rectfill_capabilities: &[&CStr] = &[c"Rect"];
static tty_feature_rectfill: tty_feature = tty_feature {
    name: c"rectfill",
    capabilities: Some(tty_feature_rectfill_capabilities),
    flags: TERM_DECFRA,
};
static tty_feature_ignorefkeys_capabilities: &[&CStr] = &[
    c"kf0@", c"kf1@", c"kf2@", c"kf3@", c"kf4@", c"kf5@", c"kf6@", c"kf7@", c"kf8@", c"kf9@",
    c"kf10@", c"kf11@", c"kf12@", c"kf13@", c"kf14@", c"kf15@", c"kf16@", c"kf17@", c"kf18@",
    c"kf19@", c"kf20@", c"kf21@", c"kf22@", c"kf23@", c"kf24@", c"kf25@", c"kf26@", c"kf27@",
    c"kf28@", c"kf29@", c"kf30@", c"kf31@", c"kf32@", c"kf33@", c"kf34@", c"kf35@", c"kf36@",
    c"kf37@", c"kf38@", c"kf39@", c"kf40@", c"kf41@", c"kf42@", c"kf43@", c"kf44@", c"kf45@",
    c"kf46@", c"kf47@", c"kf48@", c"kf49@", c"kf50@", c"kf51@", c"kf52@", c"kf53@", c"kf54@",
    c"kf55@", c"kf56@", c"kf57@", c"kf58@", c"kf59@", c"kf60@", c"kf61@", c"kf62@", c"kf63@",
];
static tty_feature_ignorefkeys: tty_feature = tty_feature {
    name: c"ignorefkeys",
    capabilities: Some(tty_feature_ignorefkeys_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_sixel_capabilities: &[&CStr] = &[c"Sxl"];
static tty_feature_sixel: tty_feature = tty_feature {
    name: c"sixel",
    capabilities: Some(tty_feature_sixel_capabilities),
    flags: TERM_SIXEL,
};
static tty_feature_progressbar_capabilities: &[&CStr] = &[c"Spb=\\E]9;4;%p1%d;%p2%d\\E\\\\"];
static tty_feature_progressbar: tty_feature = tty_feature {
    name: c"progressbar",
    capabilities: Some(tty_feature_progressbar_capabilities),
    flags: 0 as ::core::ffi::c_int,
};
static tty_feature_utf8: tty_feature = tty_feature {
    name: c"utf8",
    capabilities: None,
    flags: 0 as ::core::ffi::c_int,
};
static tty_features: [&tty_feature; 23] = {
    [
        &tty_feature_256,
        &tty_feature_appesc,
        &tty_feature_bpaste,
        &tty_feature_ccolour,
        &tty_feature_clipboard,
        &tty_feature_hyperlinks,
        &tty_feature_cstyle,
        &tty_feature_extkeys,
        &tty_feature_focus,
        &tty_feature_ignorefkeys,
        &tty_feature_margins,
        &tty_feature_mouse,
        &tty_feature_osc7,
        &tty_feature_overline,
        &tty_feature_progressbar,
        &tty_feature_rectfill,
        &tty_feature_rgb,
        &tty_feature_sixel,
        &tty_feature_strikethrough,
        &tty_feature_sync,
        &tty_feature_title,
        &tty_feature_usstyle,
        &tty_feature_utf8,
    ]
};
pub unsafe fn tty_parse_client_features(owner: &ClientRef, features: &CStr, separators: &CStr) {
    owner.parse_terminal_features(features, separators);
}
pub unsafe fn tty_parse_features(
    s: &CStr,
    sep: &CStr,
    mut enabled: *mut ::core::ffi::c_int,
    mut disabled: *mut ::core::ffi::c_int,
) {
    let mut next: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut loop_0: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut i: u_int = 0;
    let mut remove: ::core::ffi::c_int = 0;
    log_debug(format_args!("adding terminal features {}", log_cstr(s)));
    // strsep and the trailing-@ removal both write into this local copy.
    let mut copy = s.to_bytes_with_nul().to_vec();
    loop_0 = copy.as_mut_ptr().cast();
    loop {
        next = strsep(&raw mut loop_0, sep.as_ptr());
        if next.is_null() {
            break;
        }
        remove = (*next as ::core::ffi::c_int != '\0' as i32
            && *next.add(strlen(next).wrapping_sub(1 as size_t)) as ::core::ffi::c_int
                == '@' as i32) as ::core::ffi::c_int;
        if remove != 0 {
            *next.add(strlen(next).wrapping_sub(1 as size_t)) = '\0' as i32 as ::core::ffi::c_char;
        }
        i = 0 as u_int;
        while (i as usize) < tty_features.len() {
            if strcasecmp(tty_features[i as usize].name.as_ptr(), next) == 0 as ::core::ffi::c_int {
                break;
            }
            i = i.wrapping_add(1);
        }
        if i as usize == tty_features.len() {
            log_debug(format_args!(
                "unknown terminal feature: {}",
                log_cstr(CStr::from_ptr(next))
            ));
            break;
        } else if remove != 0 {
            log_debug(format_args!(
                "removing terminal feature: {}",
                crate::src::log::log_bytes(tty_features[i as usize].name.to_bytes())
            ));
            *enabled &= !((1 as ::core::ffi::c_int) << i);
            if !disabled.is_null() {
                *disabled |= (1 as ::core::ffi::c_int) << i;
            }
        } else {
            if !disabled.is_null() && *disabled & (1 as ::core::ffi::c_int) << i != 0 {
                continue;
            }
            if !*enabled & (1 as ::core::ffi::c_int) << i != 0 {
                log_debug(format_args!(
                    "adding terminal feature: {}",
                    crate::src::log::log_bytes(tty_features[i as usize].name.to_bytes())
                ));
                *enabled |= (1 as ::core::ffi::c_int) << i;
            }
        }
    }
}
pub unsafe fn tty_get_features(feat: ::core::ffi::c_int) -> CString {
    let mut names = Vec::new();
    for (i, feature) in tty_features.iter().enumerate() {
        if feat & (1 << i) != 0 {
            if !names.is_empty() {
                names.push(b',');
            }
            names.extend_from_slice(feature.name.to_bytes());
        }
    }
    CString::new(names).expect("feature names contain no NUL")
}
pub unsafe fn tty_feature_present(term: &tty_term, name: &CStr, utf8: bool) -> ::core::ffi::c_int {
    let mut tf: Option<&tty_feature> = None;
    let mut i: u_int = 0;
    if strcmp(name.as_ptr(), c"utf8".as_ptr()) == 0 as ::core::ffi::c_int {
        return utf8 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize) < tty_features.len() {
        let feature = tty_features[i as usize];
        tf = Some(feature);
        if strcmp(feature.name.as_ptr(), name.as_ptr()) == 0 as ::core::ffi::c_int {
            if term.applied_features & (1 as ::core::ffi::c_int) << i != 0 {
                return 1 as ::core::ffi::c_int;
            }
            break;
        } else {
            i = i.wrapping_add(1);
        }
    }
    let Some(tf) = tf else {
        return 0 as ::core::ffi::c_int;
    };
    if strcmp(name.as_ptr(), c"ignorefkeys".as_ptr()) == 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if tf.flags != 0 as ::core::ffi::c_int && term.flags & tf.flags != tf.flags {
        return 0 as ::core::ffi::c_int;
    }
    let Some(capabilities) = tf.capabilities else {
        return 0;
    };
    for capability in capabilities {
        let mut copy = capability.to_bytes_with_nul().to_vec();
        if let Some(equal) = copy.iter().position(|&byte| byte == b'=') {
            copy[equal] = 0;
        }
        if tty_term_has_name(term, CStr::from_ptr(copy.as_ptr().cast())) == 0 {
            return 0 as ::core::ffi::c_int;
        }
    }
    1 as ::core::ffi::c_int
}
/// Applying capability strings is component-only; Client flag publication is
/// left to the caller after releasing its terminal component guard.
#[derive(Default)]
pub struct AppliedFeatures {
    pub changed: bool,
    pub enable_utf8: bool,
}

pub unsafe fn tty_apply_features(mut term: *mut tty_term, feat: i32) -> AppliedFeatures {
    let mut result = AppliedFeatures::default();
    let mut i: u_int = 0;
    if feat == 0 as ::core::ffi::c_int {
        return result;
    }
    log_debug(format_args!(
        "applying terminal features: {}",
        log_cstr(&tty_get_features(feat))
    ));
    i = 0 as u_int;
    while (i as usize) < tty_features.len() {
        if !((*term).applied_features & (1 as ::core::ffi::c_int) << i != 0
            || !feat & (1 as ::core::ffi::c_int) << i != 0)
        {
            let tf = tty_features[i as usize];
            log_debug(format_args!(
                "applying terminal feature: {}",
                crate::src::log::log_bytes(tty_features[i as usize].name.to_bytes())
            ));
            if let Some(capabilities) = tf.capabilities {
                for capability in capabilities {
                    log_debug(format_args!(
                        "adding capability: {}",
                        crate::src::log::log_bytes(capability.to_bytes())
                    ));
                    tty_term_apply(term, capability.as_ptr(), 1 as ::core::ffi::c_int);
                }
            }
            (*term).flags |= tf.flags;
            if std::ptr::eq(tf, &tty_feature_utf8) {
                result.enable_utf8 = true;
            }
        }
        i = i.wrapping_add(1);
    }
    if (*term).applied_features | feat == (*term).applied_features {
        return result;
    }
    (*term).applied_features |= feat;
    result.changed = true;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::shared::tty::{
        tty_code, TTYC_AX, TTYC_MS, TTYC_SETAB, TTYC_SETAF, TTYC_SETRGBB, TTYC_SETRGBF,
    };
    use crate::src::tty_term::tty_term_ncodes;

    #[test]
    fn feature_application_reports_utf8_without_borrowing_a_client() {
        unsafe {
            let mut enabled = 0;
            let mut disabled = 0;
            tty_parse_features(c"utf8", c",", &mut enabled, &mut disabled);
            let mut terminal = tty_term::empty();
            assert!(terminal.client.upgrade().is_none());
            let first = tty_apply_features(&mut terminal, enabled & !disabled);
            assert!(first.changed);
            assert!(first.enable_utf8);
            let repeated = tty_apply_features(&mut terminal, enabled & !disabled);
            assert!(!repeated.changed);
            assert!(!repeated.enable_utf8);
            let none = tty_apply_features(&mut terminal, 0);
            assert!(!none.changed);
            assert!(!none.enable_utf8);
        }
    }

    #[test]
    fn feature_presence_checks_capability_names_before_values() {
        unsafe {
            let mut term = tty_term::empty();
            term.codes = vec![tty_code::default(); tty_term_ncodes() as usize].into_boxed_slice();

            term.codes[TTYC_MS as usize] = tty_code::String(Default::default());
            assert_eq!(tty_feature_present(&mut term, c"clipboard", false), 1);
            term.codes[TTYC_MS as usize] = tty_code::None;
            assert_eq!(tty_feature_present(&mut term, c"clipboard", false), 0);

            term.flags = TERM_256COLOURS | TERM_RGBCOLOURS;
            term.codes[TTYC_AX as usize] = tty_code::Flag(1);
            for code in [TTYC_SETRGBF, TTYC_SETRGBB, TTYC_SETAB, TTYC_SETAF] {
                term.codes[code as usize] = tty_code::String(Default::default());
            }
            assert_eq!(tty_feature_present(&mut term, c"RGB", false), 1);
            term.codes[TTYC_SETAF as usize] = tty_code::None;
            assert_eq!(tty_feature_present(&mut term, c"RGB", false), 0);
        }
    }
}
pub unsafe fn tty_default_features(owner: &ClientRef, name: &CStr) {
    static table: [C2RustUnnamed_35; 9] = [
        C2RustUnnamed_35 {
            name: c"mintty",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,appesc,ccolour,cstyle,extkeys,margins,overline,usstyle",
        },
        C2RustUnnamed_35 {
            name: c"tmux",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,extkeys,focus,overline,usstyle,hyperlinks,progressbar",
        },
        C2RustUnnamed_35 {
            name: c"rxvt-unicode",
            version: 0,
            features: c"256,bpaste,ccolour,cstyle,mouse,title,ignorefkeys",
        },
        C2RustUnnamed_35 {
            name: c"iTerm2",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,cstyle,extkeys,margins,usstyle,sync,osc7,hyperlinks,progressbar",
        },
        C2RustUnnamed_35 {
            name: c"foot",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,extkeys,usstyle,sync,osc7,hyperlinks",
        },
        C2RustUnnamed_35 {
            name: c"WezTerm",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,extkeys,focus,hyperlinks,margins,usstyle",
        },
        C2RustUnnamed_35 {
            name: c"ghostty",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,extkeys,focus,margins,overline,hyperlinks,osc7,sync,usstyle,progressbar",
        },
        C2RustUnnamed_35 {
            name: c"Rio",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,focus,overline,hyperlinks,osc7,sync,usstyle,progressbar",
        },
        C2RustUnnamed_35 {
            name: c"XTerm",
            version: 0,
            features: c"256,RGB,bpaste,clipboard,mouse,strikethrough,title,ccolour,cstyle,extkeys,focus",
        },
    ];
    let mut i: u_int = 0;
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_35; 9]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_35>() as usize)
    {
        if !(strcmp(table[i as usize].name.as_ptr(), name.as_ptr()) != 0 as ::core::ffi::c_int) {
            tty_parse_client_features(
                owner,
                CStr::from_ptr(table[i as usize].features.as_ptr()),
                c",",
            );
        }
        i = i.wrapping_add(1);
    }
}
