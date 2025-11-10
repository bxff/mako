#![allow(warnings)]

mod RangeHashMap;

use std::{i128::MAX, i32};

use rand::{thread_rng, Rng};

// Type aliases for better readability
type InsertPos = i32;
type Length = i32;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Op {
	/// Insert position in the document
	ins: InsertPos,
	/// Length of the operation (positive for insert, negative for delete)
	len: Length,
}

#[derive(Debug, Clone, PartialEq)]
struct OpList {
	/// List of operations to be applied
	ops: Vec<Op>,
	/// Test data for debugging (should be removed in production)
	test_op: Option<Vec<(InsertPos, Length)>>,
}


// set DTRACE = "C:\Users\dex\PC-Developement\blondie\target\release\blondie_dtrace.exe"
// https://github.com/nico-abram/blondie

/// Creates an OpList from a fixed-size array of operations
fn getOpList<const N: usize>(list: [(InsertPos, Length); N]) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Creates an OpList from a Vec of operations
fn getOpListbyVec(list: Vec<(InsertPos, Length)>) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Creates an OpList for testing with pre-existing range list
fn getOpListforTesting<const N: usize, const M: usize>(
	pre_existing_range_list: [(InsertPos, Length); N], 
	oplist: [(InsertPos, Length); M]
) -> OpList {
	OpList {
		ops: oplist.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: Some(pre_existing_range_list.into_iter().map(|(ins, len)| (ins, len) ).collect())
	}
}


#[derive(Debug, Clone, Copy)]
enum PositionRef {
	Base { base: InsertPos, index: usize },
	Insert { index: usize, offset: Length },
}

#[derive(Debug, Clone, Copy)]
enum LocateBias {
	PreferInsideInsert,
	PreferOutsideInsert,
}

impl OpList {
	fn from_oplist_to_sequential_list(&self) -> OpList {
		let mut ranges: Vec<Op> = self
			.test_op
			.as_ref()
			.map(|ops| ops.iter().map(|(ins, len)| Op { ins: *ins, len: *len }).collect())
			.unwrap_or_else(Vec::new);

		for op in &self.ops {
			if op.len == 0 {
				continue;
			}

			if op.len > 0 {
				Self::apply_insert(&mut ranges, op.ins, op.len);
			} else {
				let start = op.ins + op.len;
				let len = -op.len;
				Self::apply_delete(&mut ranges, start, len);
			}
		}

		OpList { ops: ranges, test_op: None }
	}

	fn apply_insert(ranges: &mut Vec<Op>, pos: InsertPos, len: Length) {
		if len <= 0 {
			return;
		}

		match Self::locate_position(ranges, pos, LocateBias::PreferOutsideInsert) {
			PositionRef::Insert { index, .. } => {
				ranges[index].len += len;
			}
			PositionRef::Base { base, index } => {
				Self::insert_positive(ranges, index, base, len);
			}
		}
	}

	fn apply_delete(ranges: &mut Vec<Op>, pos: InsertPos, len: Length) {
		if len <= 0 {
			return;
		}

		let mut remaining = len;
		let mut cursor = pos;

		while remaining > 0 {
			match Self::locate_position(ranges, cursor, LocateBias::PreferInsideInsert) {
				PositionRef::Insert { index, offset } => {
					let current_len = ranges[index].len;
					debug_assert!(current_len >= offset);
					let available = current_len - offset;
					let take = remaining.min(available);
					Self::shrink_insert(ranges, index, take);
					remaining -= take;
				}
				PositionRef::Base { base, index } => {
					let limit = if index < ranges.len() {
						ranges[index].ins - base
					} else {
						remaining
					};
					debug_assert!(limit > 0);
					let take = remaining.min(limit);
					Self::merge_delete(ranges, index, base, take);
					remaining -= take;
				}
			}
		}
	}

	fn shrink_insert(ranges: &mut Vec<Op>, idx: usize, len: Length) {
		if len == 0 || idx >= ranges.len() {
			return;
		}

		ranges[idx].len -= len;
		if ranges[idx].len == 0 {
			ranges.remove(idx);
		}
	}

	fn insert_positive(ranges: &mut Vec<Op>, idx: usize, base: InsertPos, len: Length) {
		if len <= 0 {
			return;
		}

		let insert_idx = idx;

		if insert_idx > 0 {
			if let Some(prev) = ranges.get_mut(insert_idx - 1) {
				if prev.len > 0 && prev.ins == base {
					prev.len += len;
					return;
				}
			}
		}

		if insert_idx < ranges.len() {
			if ranges[insert_idx].len > 0 && ranges[insert_idx].ins == base {
				ranges[insert_idx].len += len;
				return;
			}
		}

		ranges.insert(insert_idx, Op { ins: base, len });
	}

	fn merge_delete(ranges: &mut Vec<Op>, idx: usize, base: InsertPos, len: Length) {
		if len <= 0 {
			return;
		}

		if idx > 0 {
			if ranges[idx - 1].len < 0 && Self::delete_end(&ranges[idx - 1]) == base {
				ranges[idx - 1].len -= len;
				Self::try_merge_adjacent_deletes(ranges, idx - 1);
				return;
			}
		}

		if idx < ranges.len() {
			if ranges[idx].len < 0 && ranges[idx].ins == base + len {
				ranges[idx].ins = base;
				ranges[idx].len -= len;
				Self::try_merge_adjacent_deletes(ranges, idx);
				return;
			}
		}

		ranges.insert(idx, Op { ins: base, len: -len });
		Self::try_merge_adjacent_deletes(ranges, idx);
	}

	fn try_merge_adjacent_deletes(ranges: &mut Vec<Op>, mut idx: usize) {
		if idx >= ranges.len() || ranges[idx].len >= 0 {
			return;
		}

		// Merge backwards across any positives until we hit a delete whose end touches current start.
		loop {
			if idx == 0 {
				break;
			}
			let mut prev = idx - 1;
			while prev > 0 && ranges[prev].len >= 0 {
				prev -= 1;
			}
			if ranges[prev].len < 0 && Self::delete_end(&ranges[prev]) == ranges[idx].ins {
				ranges[prev].len += ranges[idx].len;
				ranges.remove(idx);
				idx = prev;
			} else {
				break;
			}
		}

		// Merge forwards, skipping over inserts until we find deletes that continue the range.
		loop {
			let mut next = idx + 1;
			while next < ranges.len() && ranges[next].len >= 0 {
				next += 1;
			}
			if next < ranges.len() && ranges[next].len < 0 && ranges[next].ins == Self::delete_end(&ranges[idx]) {
				ranges[idx].len += ranges[next].len;
				ranges.remove(next);
			} else {
				break;
			}
		}
	}

	fn locate_position(ranges: &[Op], pos: InsertPos, bias: LocateBias) -> PositionRef {
		let mut base_cursor: i64 = 0;
		let mut doc_cursor: i64 = 0;
		let target = pos as i64;

		for (index, range) in ranges.iter().enumerate() {
			let range_base = range.ins as i64;
			if range_base > base_cursor {
				let gap = range_base - base_cursor;
				if target < doc_cursor + gap {
					let base = base_cursor + (target - doc_cursor);
					return PositionRef::Base { base: base as InsertPos, index };
				}
				doc_cursor += gap;
				base_cursor = range_base;
			}

			if range.len < 0 {
				if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
					return PositionRef::Base { base: range.ins, index };
				}
				base_cursor += i64::from(-range.len);
				continue;
			} else {
				let insert_len = range.len as i64;
				if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
					return PositionRef::Base { base: range.ins, index };
				}
				if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor + insert_len {
					return PositionRef::Insert { index, offset: range.len };
				}
				if target < doc_cursor + insert_len &&
					(matches!(bias, LocateBias::PreferInsideInsert) || target > doc_cursor) {
					let offset = target - doc_cursor;
					return PositionRef::Insert { index, offset: offset as Length };
				}
				doc_cursor += insert_len;
			}
		}

		let base = base_cursor + (target - doc_cursor);
		PositionRef::Base { base: base as InsertPos, index: ranges.len() }
	}

	fn delete_end(op: &Op) -> InsertPos {
		debug_assert!(op.len < 0);
		let end = op.ins as i64 - op.len as i64;
		end as InsertPos
	}
}

fn main() {
	// let mut test_vec: OpList = getOpList([(5,-2),(4,-2)]); // 1234567 -> 12367 -> 127

	RangeHashMap::todo();

	// dbg!(test_vec.clone());
	// let mut test_vec = test_vec.from_oplist_to_sequential_list();
	// dbg!(test_vec.clone());
	// RangeHashMap::todo();

	// Say once we have two of these results, how should they combine? We can simply create a new list with the two results and call discontinues_range() on it.

	// What do we do about concurrent branches. And sequential concurrent cases too?

	// Wait first add delete.
	// For cases, --, +-, -+.
	// All resultant deletes seems to be for the original document
	// Should probably go though the code again to get a good idea of whats going on. Negative ranges is added normally to positive ranges except for when it removes the range altogether, but for the spicial case of negative case it's added differently.
	// Potentially create a list to practically test this? It doesn't need to be a text document just a long list. This is like fuzzy testing.
	
	// There is an issue with sorting prob because of arrangement of the if else statements, breaking should happen when ins is less than range ins. 
	
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_whats_already_implemented() {
		// Testing for inserting after all the ranges.

		// getOpListforTesting(getOpList(ops).from_oplist_to_sequential_list(), new_ops_to_add).from_oplist_to_sequential_list()
		//   -> equvalent to getOpList(ops + new_ops_to_add).from_oplist_to_sequential_list()

		// The state we get from_oplist_to_sequential_list is a special state
		//   -> we consider some base state, [0,inf) (we showcase this by 123456789...)
		//   -> deletes are special case here which represents deletes within [0,inf) 
		//      (e.g. [(5,-2)] would delete 6 & 7 in 123456789... always left to right)
		//   -> inserts are on top of base state, they are always in context of [0,inf),
		//      e.g. [(5,1)] would be a in 12345a6789, but note the context could also 
		//      been deleted, e.g. [(5,-1),(5,1)] would be a in 1234a6789, [(5,-2),(5,1),(6,1)] 
		//      would be a & b in 1234ab6789. By rule we take negative len first for the 
		//      same position, i.g. (5,-len) always comes before (5,+len)
		// The new_ops_to_add are added to state from_oplist_to_sequential_list by getOpListforTesting
		// getOpList also essentially creates the state by iterating over ops and changing the state one op at a time
		//   -> inserting an op to the state would require finding it's context in terms of [0,inf)
		//      e.g. for the state [(5,-1),(5,1)] would be a in 1234a6789, and if we insert [(5,1)]
		//      would be b in in 1234ab6789 and new state would be [(5,-1),(5,2)], and if we insert [(4,1)]
		//      would be b in in 1234ba6789 and new state would be [(4,1),(5,-1),(5,1)]. Note we find the 
		//      insertion position from the normal positioning, but store it in the context of [0,inf)
		//   -> deletes are similarly operated, they are found using the normal positing, i.g. 7,-1 means
		//      we are deleting the 7th position element, and we find the 7th position element in context of [0,inf)
		//      e.g. 5,-1 op onto the state of [(5,-1),(5,1)] would mean deleting a in 1234a6789, and the new
		//      state would be [(5,-1)], further if we delete again the 5,-1 op we'd get the new state of [(5,-2)]
		//   -> deletes & inserts RLE, i.g. the ops [(5,-1),(6,-1)] should always become the state [(5,-2)],
		//      a more complex example ops [(5,-2),(4,-2)] should always become the state [(2,-4)], as
		//      in 123456789 we delete 45 (as this is the normal op, left to right), then we delete
		//      36. Similerly for the state [(5,-1),(7,-1)], if we apply the op [(6,-1)], then the new state
		//      would be [(5,-3)]

        // Base = 1234567890...
        // Pre existing = 123457890...
        // After new applied (+) = 1234578+90...
		let test_vec: OpList = getOpListforTesting([(5,-1)], [(7,1)]);
		let expected_result = getOpList([(5, -1), (8, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-67890...
        // After new applied (+) = 12345-6+7890...
		let test_vec: OpList = getOpListforTesting([(5,1)], [(7,1)]);
		let expected_result = getOpList([(5, 1), (6, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test cases for: 1234567 -> 123456-7= -> 12345-=

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345+-=890...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(5,1)]);
		let expected_result = getOpList([(5,1),(5,-2),(6,1),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-+=890...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(6,1)]);
		let expected_result = getOpList([(5,-2),(6,2),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-=8+90...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(8,1)]);
		let expected_result = getOpList([(5,-2),(6,1),(7,1),(8,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		
		// Test cases for 1-2-35-

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-+35-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(4,1)]);
		let expected_result = getOpList([(1,1),(2,2),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-3+5-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(5,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,1),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35+-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(6,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,2)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35-6+7890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(8,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,1),(6,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test for 4,-4 and stuff to check for first elements. 


		// Test for delete RLE

        // Base = 1234567890...
        // Pre existing = 12345790...
        // After new applied = 1234590...
		let test_vec: OpList = getOpListforTesting([(5,-1),(7,-1)], [(6,-1)]);
		let expected_result = getOpList([(5,-3)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-=~90...
		let mut test_vec: OpList = getOpListforTesting([(3,-3),(4,1),(5,1),(6,1),(7,-1)], [(7,-1)]);
		let expected_result = getOpList([(3,-5),(4,1),(5,1),(6,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-90...
		let mut test_vec: OpList = getOpListforTesting([(3,-3),(4,1),(5,1),(6,1),(7,-1)], [(7,-3)]);
		let expected_result = getOpList([(3,-5),(4,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1234567-90...
        // After new applied = 12345690...
		let mut test_vec: OpList = getOpListforTesting([(7,1),(7,-1)], [(8,-2)]);
		let expected_result = getOpList([(6,-2)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 14678-=~90...
        // Expected = 14678-=~90...
		let mut test_vec: OpList = getOpList([(5,-1),(3,-1),(6,3),(2,-1)]); // hard to understand
		let expected_result = getOpList([(1,-2),(4,-1),(8,3)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 127890...
        // Expected = 127890...
		let mut test_vec: OpList = getOpList([(5,-2),(4,-2)]); // 1234567 -> 12367 -> 127; Testing for delete RLE within delete RLE
		let expected_result = getOpList([(2,-4)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
	}

}
