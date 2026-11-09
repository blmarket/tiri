use crate::shared::command::{cmd, cmd_list};
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct CommandRef {
    list: Rc<cmd_list>,
    group: usize,
    index: usize,
}

impl CommandRef {
    pub(super) fn iter(list: &Rc<cmd_list>) -> impl DoubleEndedIterator<Item = Self> + '_ {
        list.groups
            .iter()
            .enumerate()
            .flat_map(move |(group, commands)| {
                commands.iter().enumerate().map(move |(index, _)| Self {
                    list: list.clone(),
                    group,
                    index,
                })
            })
    }

    pub(super) fn get(&self) -> &cmd {
        &self.list.groups[self.group][self.index]
    }

    pub(super) fn same_group(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.list, &other.list) && self.group == other.group
    }
}
