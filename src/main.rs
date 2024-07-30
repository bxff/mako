use std::{iter, usize};
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
	fn discontinues_range(&self) -> OpList {
		// let mut new_oplist = getOpList([(1,0)]); // <- Start with 0 ins with 0 len
		let mut new_oplist = OpList { ops: vec![
			Op { ins: self.ops[0].ins, len: 0 }
		] }; // Start with the first element in the list with the length to be zero


		for op in self.ops.iter() {
			// Current Iins
			let mut ins = op.ins;
			let mut len = op.len;
			
			let mut new_range_element_index: isize = -1; 
			let mut delete_range_element_index: isize = -1; 
			let mut negative_range_element_index: isize = -1; 
			let mut range_len_after_addition: i32 = -1; 
			let mut range_ins_after_addition: i32 = -1; 
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				if (ins >= range.ins) && ((ins as i32) <= (range.ins as i32 +range.len)) && (range.len + len == 0) { // <- Comparing negative range to positive range and negative range removes the whole positive range.
					delete_range_element_index = i as isize;
					new_range_element_index = -1;
					break;
				} else if (ins >= range.ins) && ((ins as i32) <= (range.ins as i32 +range.len)) && (range.len + len < 0) { // <- If the element would be negative, therefore has to be handled differently.
					// negative_range_element_index = i as isize;
					// range.ins = ins;

					// This is not being invoked for 2 negative range, but is invoked for 1 negative range and 1 positive range.

					range.len += len;
					new_range_element_index = -1;
					break;
				} else if range.len < 0 && len < 0 && (ins <= range.ins) && ((ins as i32) >= (range.ins as i32 +range.len)) { // <- If we are adding two negative ranges.
					// 3,-1; 2,-1 -> 3;-2  abcd abd ab -> 4,-2 <- wait this is corret???
					// 2,-1; 2,-1 ->  abcd acd ad -> 3,-2 
					range.ins += (len*-1) as u32;
					range.len += len;
					new_range_element_index = -1;
					break;
				} 
				else if (ins >= range.ins) && ((ins as i32) <= (range.ins as i32 +range.len)) { // <- If within the range positive
					range.len += len;
					new_range_element_index = -1;
					break;
				} else if (ins < range.ins) { // <- If before the positive range
					new_range_element_index = i as isize;
					break;
				} else { // <- If after the range
					new_range_element_index = i as isize;
					new_range_element_index += 1;
				}
			}
			if delete_range_element_index != -1 {
				new_oplist.ops.remove(delete_range_element_index as usize);
			} 
			// else if range_len_after_addition < 0 && range_ins_after_addition != -1 {
			// 	dbg!(new_oplist.clone());
			// 	new_oplist.ops[delete_range_element_index as usize].ins += range_ins_after_addition as u32
			// }
			else if negative_range_element_index != -1 {
				new_oplist.ops[negative_range_element_index as usize].ins += len as u32
			}
			else if new_range_element_index != -1 { // <- If there is a new range to be inserted
				new_oplist.ops.insert(new_range_element_index as usize, Op { ins, len });
			} 
			// dbg!(new_oplist.clone());
		}

		return new_oplist;
	}

	fn prefix_sum(&mut self) {
		for i in 1..self.ops.len() {
			self.ops[i].len += self.ops[i-1].len
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

	let mut i = 0;
	while i != num_tests{
		let ins: u32 = rng.gen_range(1..=10); // Generate a random insertion position
		let len: i32 = rng.gen_range(-5..=5); // Generate a random length
		if len == 0 || (len + (ins as i32)) < 0 { i-1; continue; } // Do not include 0 len! Do not delete over a negative range!
		test_data.push(Op { ins, len });
		i += 1;
	}

	OpList { ops: test_data }
}

fn main() {
	// let mut test_vec: OpList = getOpList([(1,-1),(1,1),(1,1)]); // abcd -> bcd -> bxcd -> byxcd; one way to look at negative range might be to delete from 0 to 1 instead of 1 to -1 but it may be incorrect based on our second variable is len and we don't have any variable to consider negative.
	let mut test_vec: OpList = generate_random_test_data(1_000_000);
	// dbg!(test_vec.clone());
	let mut test_vec = test_vec.discontinues_range();
	// test_vec.prefix_sum();
	// test_vec.clearn_delete(); // For readibility
	dbg!(test_vec);

	// Say once we have two of these results, how should they combine? We can simply create a new list with the two results and call discontinues_range() on it.

	// What do we do about concurrent branches. And sequential concurrent cases too?

	// Wait first add delete.
	// For cases, --, +-, -+.
	// All resultant deletes seems to be for the original document
	// Should probably go though the code again to get a good idea of whats going on. Negative ranges is added normally to positive ranges except for when it removes the range altogether, but for the spicial case of negative case it's added differently.
	// Potentially create a list to practically test this? It doesn't need to be a text document just a long list. This is like fuzzy testing.
	
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

        assert_eq!(test_vec.discontinues_range(), expected_result);


		let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 100), (4, 1)]);
		let expected_result = getOpList([(1, 102), (3, 1)]);

		assert_eq!(test_vec.discontinues_range(), expected_result);


        let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (5, 1)]);
        let expected_result = getOpList([(1, 2), (3, 1), (5,1)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);


        let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (3, 1)]);
        let expected_result = getOpList([(1, 3), (3, 1)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);


        let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (5, 1), (7, 1)]);
        let expected_result = getOpList([(1, 1), (3, 1), (5, 1), (7, 1)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);
    }

    #[test]
    fn test_discontinues_negative_range_result() {
		let mut test_vec: OpList = getOpList([(1,1),(2,-2),(1,-1)]); // 1,1 + 2,-2 -> 1,-1 + 1,-1 -> 2,-2; abcd axbcd bcd cd
        let expected_result = getOpList([(2, -2)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);


		let mut test_vec: OpList = getOpList([(95, 18), (98, 4), (22, 44), (98, -39)]).discontinues_range();
		test_vec.prefix_sum();
		let expected_result = getOpList([(22, 44), (95, 27)]);

		assert_eq!(test_vec, expected_result);


		let mut test_vec: OpList = getOpList([(1,-1),(1,1),(1,1)]); // abcd -> bcd -> bxcd -> byxcd; one way to look at negative range might be to delete from 0 to 1 instead of 1 to -1 but it may be incorrect based on our second variable is len and we don't have any variable to consider negative
        let expected_result = getOpList([(1, -1), (1, 2)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);


		let mut test_vec: OpList = getOpList([(3,-1),(2,-1)]); // abcd -> abd -> ab
        let expected_result = getOpList([(4, -2)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);


		let mut test_vec: OpList = getOpList([(2,-1),(2,-1)]); // abcd -> acd -> ad
        let expected_result = getOpList([(3, -2)]);

        assert_eq!(test_vec.discontinues_range(), expected_result);
    }
}