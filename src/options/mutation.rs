//! Option edits whose parsing and cleanup may reenter model operations.
//! Only bounded publication/removal steps borrow the owning model.
use super::*;

fn command_parser(value: &CStr) -> cmd_parse_result {
    unsafe { cmd_parse_from_string(value, std::ptr::null_mut()) }
}

fn parse_commands(
    value: &CStr,
    parse: &mut impl FnMut(&CStr) -> cmd_parse_result,
) -> Result<Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>, CString> {
    let mut parsed = parse(value);
    if parsed.status != crate::src::shared::command::CMD_PARSE_SUCCESS {
        return Err(parsed
            .error
            .take()
            .unwrap_or_else(|| c"invalid command".to_owned()));
    }
    Ok(parsed.take_cmdlist())
}

impl OptionsScope {
    /// Set a scalar; command parsing finishes before any option component borrow.
    pub unsafe fn set_from_string(
        &self,
        definition: Option<&'static options_table_entry>,
        name: &CStr,
        value: Option<&CStr>,
        append: bool,
    ) -> Result<(), CString> {
        self.set_from_string_with_parser(definition, name, value, append, &mut command_parser)
    }

    unsafe fn set_from_string_with_parser(
        &self,
        definition: Option<&'static options_table_entry>,
        name: &CStr,
        value: Option<&CStr>,
        append: bool,
        parse: &mut impl FnMut(&CStr) -> cmd_parse_result,
    ) -> Result<(), CString> {
        if definition.is_none() {
            if !name.to_bytes().starts_with(b"@") {
                return Err(c"bad option name".to_owned());
            }
            let value = value.ok_or_else(|| c"empty value".to_owned())?;
            options_set_string(&mut self.local(), name, append as i32, |out| {
                write_cstr(out, &*value)
            });
            return Ok(());
        }
        let definition = definition.unwrap();
        if definition.type_0 != OPTIONS_TABLE_COMMAND {
            let mut cause = None;
            let result = options_from_string(
                &mut self.local(),
                definition,
                name,
                value,
                append as i32,
                &mut cause,
            );
            return if result == 0 {
                Ok(())
            } else {
                Err(cause.expect("scalar option error"))
            };
        }
        let value = value.ok_or_else(|| c"empty value".to_owned())?;
        let commands = parse_commands(value, parse)?;
        if self.entry(name).is_none() {
            self.reset_default_with_parser(definition, parse);
        }
        self.entry(name)
            .as_deref_mut()
            .map(|entry| {
                entry.value = options_value::Command(OptionCommand(commands));
            })
            .expect("command option remains installed while parsing");
        Ok(())
    }

    /// Publish an empty local array before parsing its first supplied item.
    pub unsafe fn ensure_array(&self, definition: &'static options_table_entry) {
        assert!(definition.flags & OPTIONS_TABLE_IS_ARRAY != 0);
        let name = definition.name.expect("named array definition");
        if self.entry(name).is_none() {
            options_empty(&mut self.local(), definition);
        }
    }

    pub unsafe fn clear_array(&self, name: &CStr) {
        if let Some(mut entry) = self.entry(name) {
            options_array_clear(&mut entry);
        }
    }

    /// Set/remove one item in an existing local array. A parser may consult any
    /// model; only the resulting owned command list reaches the publication step.
    pub unsafe fn set_array_item(
        &self,
        name: &CStr,
        key: &CStr,
        value: Option<&CStr>,
        append: bool,
    ) -> Result<(), CString> {
        self.set_array_item_with_parser(name, key, value, append, &mut command_parser)
    }

    unsafe fn set_array_item_with_parser(
        &self,
        name: &CStr,
        key: &CStr,
        value: Option<&CStr>,
        append: bool,
        parse: &mut impl FnMut(&CStr) -> cmd_parse_result,
    ) -> Result<(), CString> {
        let (identity, definition) = self
            .entry(name)
            .as_deref_mut()
            .map(|entry| (entry.id(), entry.tableentry))
            .expect("local array option");
        let Some(definition) =
            definition.filter(|definition| definition.flags & OPTIONS_TABLE_IS_ARRAY != 0)
        else {
            return Err(c"not an array".to_owned());
        };
        let Some(key) = options_array_correct_key(key) else {
            return Err(options_string_cause(c"bad array key: %s", &[key]));
        };
        if let Some(value) = value.filter(|_| definition.type_0 == OPTIONS_TABLE_COMMAND) {
            let commands = parse_commands(value, parse)?;
            self.entry(name)
                .as_deref_mut()
                .map(|entry| {
                    assert_eq!(entry.id(), identity, "array option replaced while parsing");
                    let ov = match options_array_get_mut(entry, &key) {
                        Some(ov) => ov,
                        None => options_array_new(entry, &key),
                    };
                    *ov = options_value::Command(OptionCommand(commands));
                })
                .expect("array option remains installed while parsing");
            return Ok(());
        }
        let mut cause = None;
        let result = self
            .entry(name)
            .as_deref_mut()
            .map(|entry| {
                assert_eq!(entry.id(), identity, "array option identity");
                options_array_set(entry, &key, value, append as i32, &mut cause)
            })
            .expect("local array option");
        if result == 0 {
            Ok(())
        } else {
            Err(cause.expect("array option error"))
        }
    }

    /// Append split values incrementally. A later parse error preserves every
    /// preceding insertion and stops before parsing any following token.
    pub unsafe fn assign_array(&self, name: &CStr, value: &CStr) -> Result<(), CString> {
        self.assign_array_with_parser(name, value, &mut command_parser)
    }

    unsafe fn assign_array_with_parser(
        &self,
        name: &CStr,
        value: &CStr,
        parse: &mut impl FnMut(&CStr) -> cmd_parse_result,
    ) -> Result<(), CString> {
        let (identity, separator) = self
            .entry(name)
            .as_deref_mut()
            .map(|entry| {
                (
                    entry.id(),
                    entry
                        .tableentry
                        .expect("array definition")
                        .separator
                        .unwrap_or(c" ,"),
                )
            })
            .expect("local array option");
        if value.to_bytes().is_empty() {
            return Ok(());
        }
        let values = if separator.to_bytes().is_empty() {
            vec![value.to_owned()]
        } else {
            value
                .to_bytes()
                .split(|byte| separator.to_bytes().contains(byte))
                .filter(|part| !part.is_empty())
                .map(|part| CString::new(part).expect("option token contains no NUL"))
                .collect()
        };
        for value in values {
            let index = self
                .entry(name)
                .as_deref_mut()
                .map(|entry| {
                    assert_eq!(
                        entry.id(),
                        identity,
                        "array assignment keeps its original entry"
                    );
                    let mut index = 0;
                    while index < UINT_MAX && options_array_get_index(entry, index).is_some() {
                        index += 1;
                    }
                    index
                })
                .expect("array option remains installed during assignment");
            if index == UINT_MAX && !separator.to_bytes().is_empty() {
                break;
            }
            let key = CString::new(index.to_string()).expect("numeric array key");
            self.set_array_item_with_parser(name, &key, Some(&value), false, parse)?;
        }
        Ok(())
    }

    /// Remove a local entry/item or reset a global built-in to its default.
    /// False means no local entry existed and callers must not push changes.
    pub unsafe fn remove_or_default(
        &self,
        name: &CStr,
        key: Option<&CStr>,
    ) -> Result<bool, CString> {
        let Some(definition) = self
            .entry(name)
            .as_deref_mut()
            .map(|entry| entry.tableentry)
        else {
            return Ok(false);
        };
        if let Some(key) = key {
            self.set_array_item(name, key, None, false)?;
        } else if let Some(definition) = definition.filter(|_| self.is_global()) {
            self.reset_default_with_parser(definition, &mut command_parser);
        } else {
            self.remove_entry(name);
        }
        Ok(true)
    }

    /// Leave the emptied entry published while its monitor is destroyed, then
    /// unlink that same identity. A reentrant replacement belongs to its caller.
    unsafe fn remove_entry(&self, name: &CStr) -> bool {
        self.remove_entry_with_cleanup(name, drop)
    }

    unsafe fn remove_entry_with_cleanup(
        &self,
        name: &CStr,
        cleanup: impl FnOnce(Option<Box<crate::src::hooks::hooks_monitor>>),
    ) -> bool {
        let Some((identity, monitor)) = self.entry(name).as_deref_mut().map(|entry| {
            entry.value = options_value::Empty;
            (entry.id(), entry.monitor_data.take())
        }) else {
            return false;
        };
        cleanup(monitor);
        let removed = match options_local_entry(&mut self.local(), name) {
            btree_map::Entry::Occupied(entry) if entry.get().id() == identity => {
                Some(entry.remove())
            }
            _ => None,
        };
        drop(removed);
        true
    }

    unsafe fn reset_default_with_parser(
        &self,
        definition: &'static options_table_entry,
        parse: &mut impl FnMut(&CStr) -> cmd_parse_result,
    ) {
        let name = definition.name.expect("named option definition");
        while self.remove_entry(name) {}
        let identity = options_empty(&mut self.local(), definition).id();
        if definition.flags & OPTIONS_TABLE_IS_ARRAY != 0 {
            if let Some(values) = definition.default_arr {
                for (index, value) in values.iter().enumerate() {
                    let key = CString::new(index.to_string()).expect("numeric array key");
                    let _ = self.set_array_item_with_parser(name, &key, Some(value), false, parse);
                }
            } else {
                let value = definition.default_str.expect("string option default");
                let _ = self.assign_array_with_parser(name, value, parse);
            }
            return;
        }
        let value = match definition.type_0 {
            OPTIONS_TABLE_STRING => options_value::String(
                definition
                    .default_str
                    .expect("string option default")
                    .to_owned(),
            ),
            OPTIONS_TABLE_COMMAND => {
                let mut parsed = parse(definition.default_str.expect("string option default"));
                if parsed.status != crate::src::shared::command::CMD_PARSE_SUCCESS {
                    return;
                }
                options_value::Command(OptionCommand(parsed.take_cmdlist()))
            }
            _ => options_value::Number(definition.default_num),
        };
        self.entry(name)
            .as_deref_mut()
            .map(|entry| {
                assert_eq!(
                    entry.id(),
                    identity,
                    "default option replaced while parsing"
                );
                entry.value = value;
            })
            .expect("default option remains installed while parsing");
    }
}
