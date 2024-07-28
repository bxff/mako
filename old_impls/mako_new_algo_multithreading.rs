use std::{iter, usize};




// type ins = i32;
// type len = i32;

#[derive(Copy, Clone, Debug)]
struct Op {
	// OpType: OpType,
	ins: u32,
	len: u32
}

#[derive(Debug)]
struct OpList {
	ops: Vec<Op> // <- Consider using arrayvec / tinyvec / smallvec if there is a huge amount of array creation / deletion.
}

fn getOpList<const N: usize>(list: [(u32, u32); N]) -> OpList {
	OpList {
        ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
    }

}

impl Default for OpList {
	fn default() -> Self {
		OpList { ops: vec![] }
	}
}

impl OpList {

	fn add(&mut self, op: Op) {
		self.ops.push(op);
	}

	fn discontinues_range(&self) -> OpList {
		let mut new_oplist = getOpList([(0,0)]); // <- Start with 0 ins with 0 len

		for (i, op) in self.ops.iter().enumerate() {
			// Current Iins
			let mut ins = op.ins;
			let mut len = op.len;
			
			let mut new_range_element_index: isize = -1;
			for range in new_oplist.ops.iter_mut() {
				if (ins > range.ins) && (len < range.ins+range.len) { // <- If within the range
					range.len += len;
					new_range_element_index = -1;
					break;
				} else if (ins < range.ins) { // <- If before the range
					new_range_element_index = i as isize;
					break;
				} else { // <- If after the range
					new_range_element_index = i as isize;
				}
			}
			if new_range_element_index != -1 { // <- If there is a new range to be inserted
				new_oplist.ops.insert(new_range_element_index as usize, Op { ins, len });
			}
		}

		return new_oplist;
	}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
		let mut test_vec: OpList = getOpList([(1,1),(3,1),(1,1),(3,1)]);
		dbg!(test_vec.discontinues_range());
		// print!("{:?}",test_vec.discontinues_range().ops)
        // assert_eq!(op_list.ops, vec![Op { ins: 1, len: 2 }]);
    }
}