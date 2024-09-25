use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// Struct representing a range as the key in the HashMap
#[derive(Debug, Clone, Copy)]
struct RangeKey {
    start: i32,
    end: i32,
}

// implement hash and eq for rangekey based on the range bounds
impl PartialEq for RangeKey {
    fn eq(&self, value: &Self) -> bool {
        // self.start == other.start && self.end == other.end
        value.start >= self.start && value.end <= self.end
    }
}

impl Eq for RangeKey {}

// impl Hash for RangeKey { fn hash<H: Hasher>(&self, state: &mut H) {
//         // self.start.hash(state);
//         // self.end.hash(state);
//     }
// }

// // Wrapper struct to allow lookup by value (i.e., to check if a value is within a range)
// #[derive(PartialEq)]
// struct ValueLookup(i32);

// // Implement Eq for ValueLookup to check if the value falls within the range
// impl PartialEq<RangeKey> for ValueLookup {
//     fn eq(&self, range: &RangeKey) -> bool {
//         range.contains(self.0)
//     }
// }

// impl Eq for ValueLookup {}

// Hash is not needed for the lookup type, since we don't use it directly as a HashMap key
// impl Hash for ValueLookup {
//     fn hash<H: Hasher>(&self, _state: &mut H) {
//         // No-op, because we only need equality checks.
//     }
// }

pub fn todo() {
    // HashMap with RangeKey as the key
    // let mut map: HashMap<RangeKey, &str> = HashMap::new();

    // // Define ranges
    // let range1 = RangeKey { start: 1, end: 5 };
    // // let range2 = RangeKey { start: 5, end: 10 };

    // // Insert ranges into the HashMap with associated values
    // map.insert(range1, "Range 1");
    // // map.insert(range2, "Range 2");

    // // Value to look up
    // let value_to_find = RangeKey { start: 1, end: 5 };
    
    // // Perform the lookup by wrapping the value into ValueLookup
    // let result = map.get_key_value(&value_to_find);
	// dbg!(result);
    
    // match result {
    //     Some((range, description)) => {
    //         println!(
    //             "Value {} belongs to range ({}, {}), which maps to '{}'", 
    //             value_to_find.start, range.start, range.end, description
    //         );
    //     }
    //     None => {
    //         println!("Value {} does not belong to any range.", value_to_find.start);
    //     }
    // }
    use std::ops::RangeInclusive;

    // Create a new HashMap for ranges
    let mut range_map: HashMap<RangeInclusive<i32>, String> = HashMap::new();

    // Mapping the range 1..2 to the value "range one to two"
    range_map.insert(1..=5, String::from("range one to two"));
    dbg!(range_map.get_key_value(&(2..=3)));

    // Output the hashmap
    for (range, value) in &range_map {
        println!("Range: {:?}, Value: {}", range, value);
    }

}
