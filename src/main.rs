use std::{iter, usize};
use rand::{thread_rng, Rng};

// type ins = i32;
// type len = i32;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Op {
	// OpType: OpType,
	ins: u32,
	len: u32
}

#[derive(Debug, Clone, PartialEq)]
struct OpList {
	ops: Vec<Op> // <- Consider using arrayvec / tinyvec / smallvec if there is a huge amount of array creation / deletion.
}

// set DTRACE = "C:\Users\dex\PC-Developement\blondie\target\release\blondie_dtrace.exe" 
// https://github.com/nico-abram/blondie

fn getOpList<const N: usize>(list: [(u32, u32); N]) -> OpList {
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
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				// dbg!(ins, len);
				// dbg!((ins >= range.ins) && (ins <= range.ins+range.len));
				// dbg!(range.clone());
				// dbg!(new_range_element_index);
				if (ins >= range.ins) && (ins <= range.ins+range.len) { // <- If within the range
					range.len += len;
					new_range_element_index = -1;
					break;
				} else if (ins < range.ins) { // <- If before the range
					new_range_element_index = i as isize;
					break;
				} else { // <- If after the range
					new_range_element_index = i as isize;
					new_range_element_index += 1;
				}
			}
			if new_range_element_index != -1 { // <- If there is a new range to be inserted
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
}

fn generate_random_test_data(num_tests: usize) -> OpList {
	let mut test_data = Vec::with_capacity(num_tests);
	let mut rng = thread_rng();

	for _ in 0..num_tests {
		let ins = rng.gen_range(1..=100); // Generate a random insertion position
		let len = rng.gen_range(1..=100); // Generate a random length
		test_data.push(Op { ins, len });
	}

	OpList { ops: test_data }
}

fn main() {
	let mut test_vec: OpList = getOpList([(1,1),(3,1),(2,1),(5,1)]);
	// let mut test_vec: OpList = generate_random_test_data(1_000_000);
	// dbg!(test_vec.clone());
	let mut test_vec = test_vec.discontinues_range();
	// test_vec.prefix_sum();
	dbg!(test_vec);

	// Say once we have two of these results, how should they combine? We can simply create a new list with the two results and call discontinues_range() on it.

	// What do we do about concurrent branches. And sequential concurrent cases too?

	// Wait first add delete.
	
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discontinues_range_result() {
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
}