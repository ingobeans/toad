pub fn pop_until<T: PartialEq>(a: &mut Vec<T>, b: &T) -> Vec<T> {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if &item == b {
            return popped;
        }
        popped.push(item);
    }
    popped
}
pub fn pop_until_not_whitespace(a: &mut Vec<char>) -> Vec<char> {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if item.is_whitespace() {
            return popped;
        }
        popped.push(item);
    }
    popped
}
pub fn pop_until_any<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> (Vec<T>, Option<T>) {
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b.contains(&item) {
            return (popped, Some(item));
        }
        popped.push(item);
    }
    (popped, None)
}
pub fn pop_until_all<T: PartialEq>(a: &mut Vec<T>, b: &[T]) -> Vec<T> {
    let mut match_index = 0;
    let mut popped = Vec::new();
    while let Some(item) = a.pop() {
        if b[match_index] == item {
            match_index += 1;
            if match_index >= b.len() {
                return popped;
            }
            continue;
        }
        match_index = 0;
        popped.push(item);
    }
    popped
}

pub fn next_is<T: PartialEq>(a: &[T], b: &T) -> bool {
    let Some(item) = a.last() else {
        return false;
    };
    item == b
}
