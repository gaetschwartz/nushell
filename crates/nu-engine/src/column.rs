use indexmap::IndexSet;
use nu_protocol::Value;
use std::collections::HashSet;

// Arbitrary threshold for when to use a hashset instead of a vector
const HASHSET_THRESHOLD: usize = 100;

pub fn get_columns(input: &[Value]) -> Vec<String> {
    let mut set: Vec<(usize, &String)> = vec![];

    let mut index = 0;
    for item in input {
        if let Value::Record { val, .. } = item {
            for col in &val.cols {
                set.push((index, col));
                index += 1;
            }
        };
    }

    // if set.len() < HASHSET_THRESHOLD {
    set.sort_by(|(_, s0), (_, s1)| s0.cmp(s1));
    set.dedup();
    set.sort_by(|(i0, _), (i1, _)| i0.cmp(i1));
    set.iter().map(|(_, s)| s.to_string()).collect()
    // } else {
    //     IndexSet::<(usize, &String)>::from_iter(set)
    //         .iter()
    //         .map(|(_, x)| x.to_string())
    //         .collect()
    // }
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
