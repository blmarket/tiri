//! Pane comparison reads owned display state before querying parent order.
use super::*;
use crate::src::shared::sort::{
    sort_criteria, SORT_ACTIVITY, SORT_CREATION, SORT_INDEX, SORT_NAME, SORT_SIZE,
};
use crate::src::shared::window::WindowWeak;
use std::cmp::Ordering;

pub(super) unsafe fn compare(
    a: &Rc<UnsafeCell<window_pane>>,
    b: &Rc<UnsafeCell<window_pane>>,
    criteria: &sort_criteria,
) -> Ordering {
    let (activity_a, id_a, size_a, parent_a, activity_b, id_b, size_b, parent_b, title_order) = {
        let a = &*a.get();
        let b = &*b.get();
        (
            a.active_point,
            a.id,
            a.sx.wrapping_mul(a.sy),
            a.window.clone(),
            b.active_point,
            b.id,
            b.sx.wrapping_mul(b.sy),
            b.window.clone(),
            crate::src::ffi::libc::strcmp(
                (*a.screen_ptr()).title.as_ptr(),
                (*b.screen_ptr()).title.as_ptr(),
            ),
        )
    };
    let order_index = |parent: &WindowWeak, observer: &std::rc::Weak<UnsafeCell<window_pane>>| {
        let window = parent.upgrade()?;
        let index = window.pane_index(observer);
        drop(window);
        index
    };
    let result = match criteria.order {
        SORT_ACTIVITY => activity_a.wrapping_sub(activity_b) as i32,
        SORT_CREATION => id_a.wrapping_sub(id_b) as i32,
        SORT_SIZE => size_a.wrapping_sub(size_b) as i32,
        SORT_INDEX => {
            let ai = order_index(&parent_a, &Rc::downgrade(a));
            let bi = order_index(&parent_b, &Rc::downgrade(b));
            (ai.is_none(), ai).cmp(&(bi.is_none(), bi)) as i32
        }
        SORT_NAME => title_order,
        _ => 0,
    };
    let result = if result == 0 { title_order } else { result };
    crate::src::sort::sort_ordering(result, criteria.reversed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_preserves_wrapping_keys_title_ties_and_reverse() {
        unsafe {
            let a = window_pane::new();
            let b = window_pane::new();
            (*a.get()).id = u32::MAX;
            (*b.get()).id = 1;
            (*a.get()).base.title = CString::new("alpha").unwrap();
            (*b.get()).base.title = CString::new("beta").unwrap();
            for order in [
                SORT_ACTIVITY,
                SORT_CREATION,
                SORT_SIZE,
                SORT_INDEX,
                SORT_NAME,
            ] {
                let mut criteria = sort_criteria {
                    order,
                    reversed: 0,
                    order_seq: &[],
                };
                assert_eq!(a.compare_for_sort(&b, &criteria), Ordering::Less);
                criteria.reversed = 1;
                assert_eq!(a.compare_for_sort(&b, &criteria), Ordering::Greater);
            }
            (*a.get()).active_point = 1;
            let criteria = sort_criteria {
                order: SORT_ACTIVITY,
                reversed: 0,
                order_seq: &[],
            };
            assert_eq!(a.compare_for_sort(&b, &criteria), Ordering::Greater);
        }
    }
}
