use indexmap::IndexSet;
use nu_protocol::Value;
use std::collections::HashSet;

// Arbitrary threshold for when to use a hashset instead of a vector
const HASHSET_THRESHOLD: usize = 200;

pub fn get_columns(input: &[Value]) -> Vec<String> {
    let tota_cols = {
        let mut total_cols = 0;
        for item in input {
            if let Value::Record { val, .. } = item {
                total_cols += val.cols.len();
            }
        }
        total_cols
    };

    if tota_cols > HASHSET_THRESHOLD {
        let mut set: IndexSet<String> = IndexSet::new();
        for item in input {
            if let Value::Record { val, .. } = item {
                for col in &val.cols {
                    set.insert(col.clone());
                }
            };
        }
        set.into_iter().collect()
    } else {
        let mut set = vec![];
        for item in input {
            if let Value::Record { val, .. } = item {
                for col in &val.cols {
                    if !set.contains(col) {
                        set.push(col.clone());
                    }
                }
            };
        }
        set
    }
}

// If a column doesn't exist in the input, return it.
pub fn nonexistent_column(inputs: &[String], columns: &[String]) -> Option<String> {
    let set: HashSet<String> = HashSet::from_iter(columns.iter().cloned());

    for input in inputs {
        if set.contains(input) {
            continue;
        }
        return Some(input.clone());
    }
    None
}
