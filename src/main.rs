use std::{iter, u32, usize};
use rand::{thread_rng, Rng};

// type ins = i32;
// type len = i32;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Op {
	// OpType: OpType,
	ins: u32,
	len: i32
}

#[derive(Debug, Clone, PartialEq)]
struct OpList {
	ops: Vec<Op> // <- Consider using arrayvec / tinyvec / smallvec if there is a huge amount of array creation / deletion.
}

// set DTRACE = "C:\Users\dex\PC-Developement\blondie\target\release\blondie_dtrace.exe" 
// https://github.com/nico-abram/blondie

fn getOpList<const N: usize>(list: [(u32, i32); N]) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
	}

}

impl OpList {
	/// Returns a new OpList with discontinues ranges, i.g. at ins x, extends the ins x till it's corrospoding length, assume an infinite length crdt edit, ins with its length will tell where such extending should take place.
	/// This also assumes that the OpList is sequential according to the time dag.
	/// 
	/// [0, inf) <- ins 1 len 100, ins 2 len 12, ins 101 len 1.
	fn from_oplist_to_sequential_list(&self) -> OpList {
		// let mut new_oplist = getOpList([(1,0)]); // <- Start with 0 ins with 0 len
		let mut new_oplist = OpList { ops: vec![
			Op { ins: self.ops[0].ins, len: 0 }
		] }; // Start with the first element in the list with the length to be zero


		// let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (5, 1)]);
		// let expected_result = getOpList([(1, 2), (2, 1), (3, 1)]);
		for op in self.ops.iter() {
			// Current Iins
			let mut op_ins = op.ins;
			let mut op_len = op.len;

			let mut aggerate_len: i32 = 0;

			let mut range_insertion_index: usize = usize::MAX;
			let mut range_insertion_OI: usize = usize::MAX;
			let mut range_insertion_len: usize = usize::MAX;

			let mut start_range: u32;
			let mut end_range: u32;

			let mut new_range = Op { ins: u32::MAX, len: i32::MAX };
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				// This may be negative...
				// Also we need to handle negate ranges.

				if range.len.is_negative() { // if we itering a negative range, we just want to check for splitting 
					// it can't really extend, more like insert in between or extend the delete.

					start_range = end_range; // previous end range, we need this as we need previous op.ins to find ending.
					end_range = range.ins + aggerate_len as u32; // so for (4,-1) we are considering 4 as the end, i.g. agg_len + 4.



					// continue from here, just need to implement delete ranges effects on postive ins, and then RLEd delete ranges.
					// test for if delete ranges effect on positive ins is correct.



					if op_ins >= start_range && op_ins <= end_range { // Split new insertion in the sequential list.
						new_range = Op {
							ins: op_ins - aggerate_len as u32,
							len: op_len
						};
						range_insertion_index = i;
						break;	
					} else { // if op_ins > start_range && op_ins < end_range i.g. op is after the range
						new_range = Op {
							ins: op_ins - aggerate_len as u32,
							len: op_len
						};
						range_insertion_index = i+1; // Just insert after the last range
					}

					aggerate_len += range.len;
					break;
				}

				start_range = range.ins + aggerate_len as u32; // considering it as postive for positive ranges for now
				end_range = start_range + range.len as u32; // considering it as postive for positive ranges for now

				if op_ins >= start_range && op_ins <= end_range { // Add to range
					range.len += op_len;
					range_insertion_index = usize::MAX;
					break;
				} else if op_ins < start_range { // Split new insertion in the sequential list.
					new_range = Op {
						ins: op_ins - aggerate_len as u32,
						len: op_len
					};
					range_insertion_index = i;
					break;
				} else { // if op_ins > start_range && op_ins < end_range i.g. op is after the range
					new_range = Op {
						ins: op_ins - aggerate_len as u32,
						len: op_len
					};
					range_insertion_index = i+1; // Just insert after the last range
				}

				aggerate_len += range.len;
			} 

			if range_insertion_index != usize::MAX {
				assert!(new_range != Op { ins: u32::MAX, len: i32::MAX }); // this is more or less a placeholder.
				new_oplist.ops.insert(range_insertion_index as usize, new_range)
			}
		}

		// todo!();
		return new_oplist;
	}

	/// Convert sequential_list into a oplist
	fn from_sequential_list_to_oplist(&mut self) {
		// incorrect
		for i in 1..self.ops.len() {
			self.ops[i].ins += self.ops[i-1].len as u32;
		}
	}

	/// Changes delete ranges such as 2,-1 to 1,2 to 1,-2.
	/// Should only be used for reading output for testing.
	fn clearn_delete(&mut self) {
		for op in self.ops.iter_mut() {
			if op.len < 0 {
				let ins = op.ins;
				op.ins = (op.ins as i32 + op.len) as u32;
				op.len = -(ins as i32);
			}
		}
	}
	
	/// Human readable output for testing.
	fn clearn_output(&mut self) {
		todo!(); // maybe not worthwhile
	}
}

// abcd - ins 1 => bcd - ins 1 => cd 

/// Generating random test data for testing, but is not implemented correctly yet, spicifically includsive range is random. 
/// This is very poorly optimized but shouldn't matter for testing.
fn generate_random_test_data(num_tests: usize) -> OpList {
	let mut test_data = Vec::with_capacity(num_tests);
	let mut rng = thread_rng();

	let mut i: i32 = 0;
	while i != num_tests as i32{
		let ins: u32 = rng.gen_range(1..=10); // Generate a random insertion position
		let len: i32 = rng.gen_range(-5..=5); // Generate a random length
		if len == 0 || (len + (ins as i32)) < 0 { i-1; continue; } // Do not include 0 len! Do not delete over a negative range!
		test_data.push(Op { ins, len });
		i += 1;
	}

	OpList { ops: test_data }
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

	OpList { ops: test_data }
}


fn main() {
	// let mut test_vec: OpList = getOpList([(1,-1),(1,1),(1,1)]); // abcd -> bcd -> bxcd -> byxcd; one way to look at negative range might be to delete from 0 to 1 instead of 1 to -1 but it may be incorrect based on our second variable is len and we don't have any variable to consider negative.
	// let mut test_vec: OpList = generate_random_test_data(4);
	let mut test_vec: OpList = generate_only_positive_random_test_data(4);
	let mut test_vec: OpList = getOpList([(5,1), (7,1), (9,7), (5,6)]);
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
	fn test_discontinues_positive_range_result() {
		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (4, 1)]);
		let expected_result = getOpList([(1, 2), (3, 2)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		// The idea of this sort of list came from the SIMD implementation, len which is what the sorted list is to be compared to represents the original length of the document. If the elements are within the length of the document then they should be added.
		// The idea is that need to know for each ins position, what is the aggerate length it adds, which is what this exactly finds and also within a single ins we need to find which elements are extending to find figureout concurrent possition.
		// So the the resultent should be [(index,len)] where the index represents the position of insertion with the context of the original document, and length represents the length of the insertions.
		// Deletions are a bit complicated, deletions within the newly inserted documents should be reletively easy, but for the original document isn't very easy. New branch (sequential ops) delete should simply detuct the len. 
		// 2,1;1,1 -> -b-a- -> 1,1;2,1
		// 1,1;3,1;1,100;4,1;105,1 -> -100ca-db-- -> 1,102;2,1
		// 1,1;3,1;5,1;7,1 -> -a-b-c-d- -> 1,1;2,1;3,1;4,1 <- taking this as an example, depending on the length of the document we may or may not wana add ins7.
		// 1,1;3,1;1,1;1,3 -> -cad-b- -> 1,3;2,1
		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 100), (4, 1), (105,1)]); // <- 104 should technically extend 3? Maybe not? -100ca-b-- d can be extend b at 105/106; therefore compared to [0,inf), 1,102;2,2; I think so its slightly different.
		let expected_result = getOpList([(1, 102), (3, 1)]); // <- 3 is no longer possible to extend, the idea being that 1 extends to 102, but doesn't extend 3 which doesn't extend anything that isn't extended by 1?
		
		dbg!(test_vec.clone().from_oplist_to_sequential_list());

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (5, 1)]);
		let expected_result = getOpList([(1, 2), (3, 1), (5,1)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (3, 1)]);
		let expected_result = getOpList([(1, 3), (3, 1)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (5, 1), (7, 1)]);
		let expected_result = getOpList([(1, 1), (3, 1), (5, 1), (7, 1)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
	}

	#[test]
	fn test_discontinues_negative_range_result() {
		let mut test_vec: OpList = getOpList([(1,1),(2,-2),(1,-1)]); // 1,1 + 2,-2 -> 1,-1 + 1,-1 -> 2,-2; abcd axbcd bcd cd
		let expected_result = getOpList([(2, -2)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(95, 18), (98, 4), (22, 44), (98, -39)]).from_oplist_to_sequential_list();
		// test_vec.prefix_sum();
		let expected_result = getOpList([(22, 44), (95, 27)]);

		assert_eq!(test_vec, expected_result);


		let mut test_vec: OpList = getOpList([(1,-1),(1,1),(1,1)]); // abcd -> bcd -> bxcd -> byxcd; one way to look at negative range might be to delete from 0 to 1 instead of 1 to -1 but it may be incorrect based on our second variable is len and we don't have any variable to consider negative
		let expected_result = getOpList([(1, -1), (1, 2)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(3,-1),(2,-1)]); // abcd -> abd -> ad
		let expected_result = getOpList([(3, -2)]); // Should be 3,-2

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);


		let mut test_vec: OpList = getOpList([(2,-1),(2,-1)]); // abcd -> acd -> ad
		let expected_result = getOpList([(3, -2)]);

		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
	}


	#[test]
	fn test_discontinues_sorting() {
		todo!() // The ins should be sorted.
	}
}