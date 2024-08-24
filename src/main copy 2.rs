use rand::{thread_rng, Rng};


// type ins = i32;
// type len = i32;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Op {
	// OpType: OpType,
	ins: i32, // i32 is used as other wise there is a lot of conversions, its better to expose the API as u32.
	len: i32,
}

#[derive(Debug, Clone, PartialEq)]
struct OpList {
	ops: Vec<Op>, // <- Consider using arrayvec / tinyvec / smallvec if there is a huge amount of array creation / deletion.
	test_op: Option<Vec<(i32, i32)>>, // For testing, to be removed
}

// set DTRACE = "C:\Users\dex\PC-Developement\blondie\target\release\blondie_dtrace.exe" 
// https://github.com/nico-abram/blondie

fn getOpList<const N: usize>(list: [(i32, i32); N]) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Same as getOpList, just takes Vec as input
fn getOpListbyVec(list: Vec<(i32, i32)>) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Same as getOpList, just for testing converts the test_op as already existing range list
fn getOpListforTesting<const N: usize, const M: usize>(pre_existing_range_list: [(i32, i32); N], oplist: [(i32, i32); M]) -> OpList {
	OpList {
		ops: oplist.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: Some(pre_existing_range_list.into_iter().map(|(ins, len)| (ins, len) ).collect())
	}
}

impl OpList {
	/// Returns a new OpList with discontinues ranges, i.g. at ins x, extends the ins x till it's corrospoding length, assume an infinite length crdt edit, ins with its length will tell where such extending should take place.
	/// This also assumes that the OpList is sequential according to the time dag.
	/// 
	/// [0, inf) <- ins 1 len 100, ins 2 len 12, ins 101 len 1.
	fn from_oplist_to_sequential_list(&self) -> OpList {
		// Use test data if provided for the range_list (i.g. new_oplist)
		let mut new_oplist: OpList = OpList { ops: vec![], test_op: None };
		if self.test_op.is_some() {
			new_oplist = getOpListbyVec(self.test_op.clone().unwrap());
		} else {
			new_oplist = OpList { ops: vec![
				Op { ins: self.ops[0].ins, len: 0 },
			],test_op: None }; // Start with the first element in the list with the length to be zero

			// let mut new_oplist = getOpList([(1,1),(2,1),(4,-1),(5,1)]);
			// let mut new_oplist = getOpList([(5,-2),(6,1),(7,1)]);
		}

		// let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (5, 1)]);
		// let expected_result = getOpList([(1, 2), (2, 1), (3, 1)]);
		for op in self.ops.iter() {
			// Current Iins
			let mut op_ins = op.ins;
			let mut op_len = op.len;

			if op_len.is_negative() {
				// convert 5,-2 to 3,-2 for comparing ins with start and end range
				op_ins = op_ins + op_len;
			}

			let mut aggerate_len: i32 = 0;

			let mut range_insertion_index: usize = usize::MAX;
			let mut range_delete_index: usize = usize::MAX;

			let mut start_range: i32;
			let mut end_range: i32 = i32::MAX;

			let mut new_range = Op { ins: i32::MAX, len: i32::MAX };
			let mut orignal_doc_delete_range = Op { ins: i32::MAX, len: 0 }; // basically this delete keeps track of original document deletes which are being extended.

			// required for fixing elements which are already deleted in the orignal and are extended.
			let mut previous_delete_range_start: i32 = i32::MAX;
			let mut previous_delete_range_end: i32 = i32::MAX;
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				if range.len.is_negative() { // if we itering a negative range, we just want to check for splitting 

					// This block is basically checking for split insertions just before a negative range.
					// it can't really extend, more like insert in between or extend the delete.

					// 7,-2 -> 7,9
					previous_delete_range_start = range.ins;
					previous_delete_range_end = previous_delete_range_start + (-range.len);


					if end_range != i32::MAX {
						start_range = end_range; // previous end range, we need this as we need previous op.ins to find ending.
					} else { start_range = 0 }
					if (range.ins as i32 + aggerate_len) > 0 { // When is this not true? Delete start + agg_len > 0 means that delete range is going back.
						end_range = (range.ins as i32 + aggerate_len); // so for (4,-1) we are considering 4 as the end, i.g. agg_len + 4.
					} else {
						panic!("Deletes should delete into the negative range, e.g. (4,-5) shouldn't exists.");
						// end_range = range.ins 
					}
					// dbg!(start_range);
					// dbg!(end_range);

					if op_ins >= start_range && op_ins <= end_range { // Split new insertion in the sequential list.
						if op_len.is_positive() {
							new_range = Op {
								ins: op_ins - aggerate_len,
								len: op_len
							};
							range_insertion_index = i; // If we are inserting within a deleted range, we would want the insert to go after the delete; nah its afterwords would be handled ahead
							break;	
						} else {
							match handle_orignal_doc_deletes(&mut op_ins, &mut op_len, &mut aggerate_len, &mut end_range, i, &mut range_delete_index, &mut orignal_doc_delete_range) {
								BreakOrContinue::Break => break,
								BreakOrContinue::ContinueToAggLenAdd => {},
							}
						}
					} else { // if op_ins > start_range && op_ins < end_range i.g. op is after the range
						new_range = Op {
							ins: op_ins - aggerate_len - range.len, // basically if we are inserting after the range, we would also want the range len included in aggerate len
							len: op_len
						};
						range_insertion_index = i+1; // Just insert after the last range
					}

					aggerate_len += range.len;
					continue;
				}

				// Insertions which extend deleted elements, i.g. [0,inf) is extended by (1,1) and (2,1) but [1,2] is deleted on the original list.
				// if range.ins > /* Not >= as it would always be more */ previous_delete_range_ins && range.ins <= (previous_delete_range_ins + (-previous_delete_range_len as u32)) {
				if range.ins > previous_delete_range_start && range.ins <= previous_delete_range_end {

					// This block basically changes start positions of ranges whose original length has been deleted.
					
					// 7,-2 -> 7 + 2 + (agg_len-2)
					// [(5,-2),(6,len),(7,len)]
					start_range = (previous_delete_range_start + (previous_delete_range_end - previous_delete_range_start) + aggerate_len);
				} else { start_range = range.ins + aggerate_len; } // considering it as postive for positive ranges for now
				end_range = start_range + range.len; // considering it as postive for positive ranges for now

				// Possibly could proof check here?
				// dbg!(start_range);
				// dbg!(end_range);

				if op_ins >= start_range && op_ins <= end_range { // Adds to the range in the sequential list.
					range.len += op_len;
					range_insertion_index = usize::MAX;
					break;
				} else if op_ins < start_range { // Split new insertion in the sequential list.
					// This shouldn't be possible within original length deleted elements as we can't be before the start range as we manually iter throught it before.
					// [(5,-2),(6,len),(7,len)] as we check first 0 to 3 then 6s and 7s length.
					new_range = Op {
						ins: op_ins - aggerate_len,
						len: op_len
					};
					range_insertion_index = i;
					break;
				} else { // if op_ins > start_range && op_ins < end_range i.g. OP is after the range
					new_range = Op {
						ins: op_ins - aggerate_len - range.len, // basically if we are inserting after the range, we would also want the range len included in aggerate len
						len: op_len
					};
					range_insertion_index = i+1; // Just insert after the last range
				}

				aggerate_len += range.len;
			} 

			if range_insertion_index != usize::MAX {
				assert!(new_range != Op { ins: i32::MAX, len: i32::MAX }); // this is more or less a placeholder.
				new_oplist.ops.insert(range_insertion_index as usize, new_range)
			}
		}

		return new_oplist;
	}

	/// Convert sequential_list into a oplist
	fn from_sequential_list_to_oplist(&mut self) {
		// incorrect
		for i in 1..self.ops.len() {
			self.ops[i].ins += self.ops[i-1].len;
		}
	}

	/// Changes delete ranges such as 2,-1 to 1,2 to 1,-2.
	/// Should only be used for reading output for testing.
	fn clearn_delete(&mut self) {
		for op in self.ops.iter_mut() {
			if op.len < 0 {
				let ins = op.ins;
				op.ins = (op.ins as i32 + op.len);
				op.len = -(ins as i32);
			}
		}
	}
	
	/// Human readable output for testing.
	fn clearn_output(&mut self) {
		todo!(); // maybe not worthwhile
	}
}

enum BreakOrContinue {
    Break,
    ContinueToAggLenAdd,
}

enum DeleteHandler {
	DeleteExtending(i32, i32),
	DeleteEnds
}

/// Helper function for handling deletes to the orignial document which may be extending.
/// This code basically extends a delete which is being iterated.
fn handle_orignal_doc_deletes(
    op_ins: &mut i32,
    op_len: &mut i32,
    aggerate_len: &mut i32,
    end_range: &mut i32,
    i: usize,
    // i: &mut usize,
    range_delete_index: &mut usize,
    orignal_doc_delete_range: &mut Op,
) -> BreakOrContinue {

	if *op_ins - *op_len - *aggerate_len > *end_range { // delete extends beyond end of range, therefor we continue to find deletes.
		if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
			orignal_doc_delete_range.ins = *op_ins - *aggerate_len; // delete start
			orignal_doc_delete_range.len = *end_range - orignal_doc_delete_range.ins; // end range - delete start
			
			*op_ins = *end_range; // end range
			*op_len = *op_ins - *op_len - *aggerate_len - *end_range; // delete end - end range
		} else { // If delete_range already exists
			orignal_doc_delete_range.len += *end_range - (*op_ins - *aggerate_len); // end range - delete start
			*op_ins = *end_range;
			*op_len = *op_ins - *op_len - *aggerate_len - *end_range; // delete end - endrange
		}
	} else { // delete doesn't extends beyond end of range
		if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
			orignal_doc_delete_range.ins = *op_ins - *aggerate_len; // delete start
			orignal_doc_delete_range.len = *op_ins - *op_len - *aggerate_len; // delete end
			*range_delete_index = i;
			return BreakOrContinue::Break	
		} else {
			orignal_doc_delete_range.len += *op_ins - *op_len - *aggerate_len; // delete end
			*range_delete_index = i;
			return BreakOrContinue::Break	
		}
	}
	return BreakOrContinue::ContinueToAggLenAdd;
}

// abcd - ins 1 => bcd - ins 1 => cd 

/// Generating random test data for testing, but is not implemented correctly yet, spicifically includsive range is random. 
/// This is very poorly optimized but shouldn't matter for testing.
fn generate_random_test_data(num_tests: usize) -> OpList {
	let mut test_data = Vec::with_capacity(num_tests);
	let mut rng = thread_rng();

	let mut i: i32 = 0;
	while i != num_tests as i32{
		let ins: i32 = rng.gen_range(1..=10); // Generate a random insertion position
		let len: i32 = rng.gen_range(-5..=5); // Generate a random length
		if len == 0 || (len + (ins as i32)) < 0 { i-1; continue; } // Do not include 0 len! Do not delete over a negative range!
		test_data.push(Op { ins, len });
		i += 1;
	}

	OpList { ops: test_data, test_op: None }
}


/// Old code: generating only positive random test data for testing. 
fn generate_only_positive_random_test_data(num_tests: usize) -> OpList {
	let mut test_data = Vec::with_capacity(num_tests);
	let mut rng = thread_rng();

	for _ in 0..num_tests {
		let ins = rng.gen_range(1..=10); // Generate a random insertion position
		let len = rng.gen_range(1..=10); // Generate a random length
		test_data.push(Op { ins, len });
	}

	OpList { ops: test_data, test_op: None }
}


fn main() {
	// let mut test_vec: OpList = getOpList([(1,-1),(1,1),(1,1)]); // abcd -> bcd -> bxcd -> byxcd; one way to look at negative range might be to delete from 0 to 1 instead of 1 to -1 but it may be incorrect based on our second variable is len and we don't have any variable to consider negative.
	// let mut test_vec: OpList = generate_random_test_data(4);
	let mut test_vec: OpList = generate_only_positive_random_test_data(4);
	let mut test_vec: OpList = getOpList([(5,1), (7,1), (9,7), (5,6)]);
	// let mut test_vec: OpList = getOpList([]);
	// let mut test_vec: OpList = getOpList([(4,1)]);

	// let mut test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(8,1)]);
	// let mut test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(8,1)]);
	dbg!(test_vec.clone());
	let mut test_vec = test_vec.from_oplist_to_sequential_list();
	// test_vec.prefix_sum();
	// test_vec.clearn_delete(); // For readibility
	dbg!(test_vec.clone());
	// dbg!(test_vec.from_oplist_to_sequential_list());

	// Say once we have two of these results, how should they combine? We can simply create a new list with the two results and call discontinues_range() on it.

	// What do we do about concurrent branches. And sequential concurrent cases too?

	// Wait first add delete.
	// For cases, --, +-, -+.
	// All resultant deletes seems to be for the original document
	// Should probably go though the code again to get a good idea of whats going on. Negative ranges is added normally to positive ranges except for when it removes the range altogether, but for the spicial case of negative case it's added differently.
	// Potentially create a list to practically test this? It doesn't need to be a text document just a long list. This is like fuzzy testing.
	
	// There is an issue with sorting prob because of arrangement of the if else statements, breaking should happen when ins is less than range ins. 
	
}

// [src/main.rs:151:2] test_vec.clone() = OpList {
//     ops: [
//         Op {
//             ins: 5,
//             len: -1,
//         },
//         Op {
//             ins: 3,
//             len: -1,
//         },
//         Op {
//             ins: 6,
//             len: 3,
//         },
//         Op {
//             ins: 2,
//             len: -1,
//         },
//     ],
// }
// [src/main.rs:155:2] test_vec = OpList {
//     ops: [
//         Op {
//             ins: 4, <- IS INCORRECT< SHOULD BE 3, -2; abcd abd ad
//             len: -2,
//         },
//         Op {
//             ins: 5,
//             len: -1,
//         },
//         Op {
//             ins: 6,
//             len: 3,
//         },
//     ],
// }

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_whats_already_implemented() {
		// Testing for inserting after all the ranges.
		let mut test_vec: OpList = getOpListforTesting([(5,-1)], [(7,1)]);
		let expected_result = getOpList([(5, -1), (8, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(5,1)], [(7,1)]);
		let expected_result = getOpList([(5, 1), (6, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test cases for: 1234567 -> 123456-7= -> 12345-=
		let mut test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(5,1)]);
		let expected_result = getOpList([(5,1),(5,-2),(6,1),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(6,1)]);
		let expected_result = getOpList([(5,-2),(6,2),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(8,1)]);
		let expected_result = getOpList([(5,-2),(6,1),(7,1),(8,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		
		// Test cases for 1-2-35-
		let mut test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(4,1)]);
		let expected_result = getOpList([(1,1),(2,2),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(5,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,1),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(6,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,2)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		let mut test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(8,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,1),(6,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test for 4,-4 and stuff to check for first elements. 
	}

}