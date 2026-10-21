//! Pane comparison reads owned display state before querying parent order.
use super::*;
use crate::shared::sort::{
    sort_criteria, SORT_ACTIVITY, SORT_CREATION, SORT_INDEX, SORT_NAME, SORT_SIZE,
};
use crate::shared::window::WindowWeak;
use std::cmp::Ordering;

pub(super) unsafe fn compare(
    a: &Rc<RefCell<window_pane>>,
    b: &Rc<RefCell<window_pane>>,
    criteria: &sort_criteria,
) -> Ordering {
    let (activity_a, id_a, size_a, parent_a, activity_b, id_b, size_b, parent_b, title_order) = {
        let a = &*a.as_ptr();
        let b = &*b.as_ptr();
        (
            a.active_point,
            a.id,
            a.sx.wrapping_mul(a.sy),
            a.window.clone(),
            b.active_point,
            b.id,
            b.sx.wrapping_mul(b.sy),
            b.window.clone(),
            a.with_screen(|a| b.with_screen(|b| a.title.cmp(&b.title) as i32))
                .flatten()
                .expect("displayed pane screens"),
        )
    };
    let order_index = |parent: &WindowWeak, observer: &std::rc::Weak<RefCell<window_pane>>| {
        let window = parent.upgrade()?;
        let index = window.pane_index(observer);
        drop(window);
        index
    };
    let result = match criteria.order {
        SORT_ACTIVITY => activity_a.cmp(&activity_b) as i32,
        SORT_CREATION => id_a.cmp(&id_b) as i32,
        SORT_SIZE => size_a.cmp(&size_b) as i32,
        SORT_INDEX => {
            let ai = order_index(&parent_a, &Rc::downgrade(a));
            let bi = order_index(&parent_b, &Rc::downgrade(b));
            (ai.is_none(), ai).cmp(&(bi.is_none(), bi)) as i32
        }
        SORT_NAME => title_order,
        _ => 0,
    };
    let result = if result == 0 { title_order } else { result };
    crate::sort::sort_ordering(result, criteria.reversed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_orders_whole_keys_title_ties_and_reverse() {
        unsafe {
            let a = window_pane::new();
            let b = window_pane::new();
            (*a.as_ptr()).id = 1;
            (*b.as_ptr()).id = u32::MAX;
            (*a.as_ptr()).base.title = CString::new("alpha").unwrap();
            (*b.as_ptr()).base.title = CString::new("beta").unwrap();
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
            (*a.as_ptr()).active_point = 1;
            let criteria = sort_criteria {
                order: SORT_ACTIVITY,
                reversed: 0,
                order_seq: &[],
            };
            assert_eq!(a.compare_for_sort(&b, &criteria), Ordering::Greater);
        }
    }
}
