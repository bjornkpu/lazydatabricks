//! A list with a cursor. The selection is `Option<usize>` and is `None` exactly when the list
//! is empty, so callers never index.

/// A cursor movement, independent of which list it lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selectable<T> {
    items: Vec<T>,
    selected: Option<usize>,
}

impl<T> Default for Selectable<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: None,
        }
    }
}

impl<T> Selectable<T> {
    /// A list with the cursor on its first item.
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        let mut list = Self::default();
        list.set_items(items);
        list
    }

    /// Puts `item` first. The cursor keeps its index, so a cursor on the first row now sits on
    /// the newcomer, which is what an optimistic insert wants.
    pub fn push_front(&mut self, item: T) {
        self.items.insert(0, item);
        if self.selected.is_none() {
            self.selected = Some(0);
        }
    }

    pub const fn items_mut(&mut self) -> &mut [T] {
        self.items.as_mut_slice()
    }

    /// Replaces the items, keeping the cursor where it was if that is still a valid position.
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.selected = self
            .last_index()
            .map(|last| self.selected.unwrap_or(0).min(last));
    }

    pub const fn items(&self) -> &[T] {
        self.items.as_slice()
    }

    pub const fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.selected?)
    }

    /// Moves the cursor to the first item matching `pred`, if any; otherwise leaves it alone.
    pub fn select_where(&mut self, pred: impl FnMut(&T) -> bool) {
        if let Some(index) = self.items.iter().position(pred) {
            self.selected = Some(index);
        }
    }

    pub fn apply(&mut self, movement: Move) {
        let Some(last) = self.last_index() else {
            return;
        };
        self.selected = Some(match (movement, self.selected.unwrap_or(0)) {
            (Move::Up, current) => current.saturating_sub(1),
            (Move::Down, current) => current.saturating_add(1).min(last),
            (Move::First, _) => 0,
            (Move::Last, _) => last,
        });
    }

    /// `n of m`, as shown bottom-right of a panel. `0 of 0` when empty.
    pub fn counter(&self) -> String {
        let n = self.selected.map_or(0, |i| i.saturating_add(1));
        format!("{n} of {}", self.items.len())
    }

    const fn last_index(&self) -> Option<usize> {
        self.items.len().checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Selectable<&'static str> {
        let mut list = Selectable::default();
        list.set_items(vec!["a", "b", "c"]);
        list
    }

    #[test]
    fn empty_has_no_selection_and_ignores_moves() {
        let mut list: Selectable<&str> = Selectable::default();
        list.apply(Move::Down);
        assert_eq!(list.selected_index(), None);
        assert_eq!(list.selected(), None);
        assert_eq!(list.counter(), "0 of 0");
    }

    #[test]
    fn new_and_push_front() {
        let mut list = Selectable::new(vec!["b", "c"]);
        assert_eq!(list.selected(), Some(&"b"));
        list.apply(Move::Down);
        list.push_front("a");
        assert_eq!(list.items(), ["a", "b", "c"]);
        assert_eq!(list.selected(), Some(&"b"), "cursor index kept");
        let mut empty: Selectable<&str> = Selectable::default();
        empty.push_front("x");
        assert_eq!(empty.selected(), Some(&"x"));
    }

    #[test]
    fn set_items_selects_first() {
        let list = three();
        assert_eq!(list.selected(), Some(&"a"));
        assert_eq!(list.counter(), "1 of 3");
    }

    #[test]
    fn down_and_up_clamp() {
        let mut list = three();
        list.apply(Move::Up);
        assert_eq!(list.selected_index(), Some(0));
        for _ in 0..5 {
            list.apply(Move::Down);
        }
        assert_eq!(list.selected_index(), Some(2));
        assert_eq!(list.counter(), "3 of 3");
    }

    #[test]
    fn first_and_last() {
        let mut list = three();
        list.apply(Move::Last);
        assert_eq!(list.selected(), Some(&"c"));
        list.apply(Move::First);
        assert_eq!(list.selected(), Some(&"a"));
    }

    #[test]
    fn select_where_finds_or_keeps() {
        let mut list = three();
        list.select_where(|item| *item == "c");
        assert_eq!(list.selected_index(), Some(2));
        list.select_where(|item| *item == "zzz");
        assert_eq!(list.selected_index(), Some(2));
    }

    #[test]
    fn set_items_keeps_cursor_but_clamps_to_new_length() {
        let mut list = three();
        list.apply(Move::Last);
        list.set_items(vec!["x", "y"]);
        assert_eq!(list.selected(), Some(&"y"));
        list.set_items(vec![]);
        assert_eq!(list.selected_index(), None);
    }
}
