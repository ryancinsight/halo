
#[cfg(test)]
mod tests {
    use crate::collections::other::deque::BrandedDeque;
    use crate::GhostToken;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug)]
    struct DropTracker {
        id: i32,
        drops: Rc<RefCell<Vec<i32>>>,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            self.drops.borrow_mut().push(self.id);
        }
    }

    #[test]
    fn test_branded_deque_double_drop() {
        let drops = Rc::new(RefCell::new(Vec::new()));

        {
            GhostToken::new(|mut token| {
                let mut deque: BrandedDeque<'_, DropTracker, 4> = BrandedDeque::new();
                deque.push_back(DropTracker { id: 1, drops: drops.clone() });

                // Pop it. This moves it out.
                let item = deque.pop_back().unwrap();
                // Item is dropped here (at end of scope)
                // If Deque also drops it, we see double drop for id 1.
            });
            // Deque is dropped here.
        }

        let recorded_drops = drops.borrow();
        println!("Drops: {:?}", recorded_drops);
        // If correct: [1]
        // If double drop: [1, 1] (or panic/segfault)

        // Count occurrences of 1
        let count_1 = recorded_drops.iter().filter(|&&x| x == 1).count();
        assert_eq!(count_1, 1, "Double drop detected!");
    }
}
