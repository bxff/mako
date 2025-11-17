#![allow(warnings)]

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

/// Builds an `OpList` from a fixed-size array of (position, length) tuples while clearing any testing state.
fn getOpList<const N: usize>(list: [(InsertPos, Length); N]) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Builds an `OpList` from a runtime `Vec` of (position, length) tuples while clearing any testing state.
fn getOpListbyVec(list: Vec<(InsertPos, Length)>) -> OpList {
	OpList {
		ops: list.into_iter().map(|(ins, len)| Op{ ins, len }).collect(),
		test_op: None
	}
}

/// Builds an `OpList` and seeds it with a pre-existing sequential list for testing.
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

enum DeleteEmit {
	Existing(Op),
	DocSpan { base_start: i64, len: i64 },
}

impl OpList {
	/// Replays the operations in order to produce a sequential list of ranges anchored to the base document.
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

	fn from_sequential_list_to_oplist(&mut self) {
		let mut base_cursor: i64 = 0;
		let mut doc_cursor: i64 = 0;
		let mut write_idx: usize = 0;

		for read_idx in 0..self.ops.len() {
			let range = self.ops[read_idx];
			if range.len == 0 {
				continue;
			}

			let range_base = i64::from(range.ins);
			if range_base > base_cursor {
				let advance = range_base - base_cursor;
				doc_cursor += advance;
				base_cursor = range_base;
			}

			if range.len > 0 {
				let ins: InsertPos = doc_cursor.try_into().expect("insert cursor overflow");
				Self::write_op(&mut self.ops, write_idx, Op { ins, len: range.len });
				write_idx += 1;
				doc_cursor += i64::from(range.len);
			} else {
				let delete_len = -i64::from(range.len);
				let delete_start = doc_cursor;
				let ins: InsertPos = (delete_start + delete_len)
					.try_into()
					.expect("delete cursor overflow");
				let len: Length = delete_len.try_into().expect("delete len overflow");
				Self::write_op(&mut self.ops, write_idx, Op { ins, len: -len });
				write_idx += 1;
				base_cursor += delete_len;
			}
		}

		self.ops.truncate(write_idx);
		self.test_op = None;
	}

	/// Merges another sequential list into `self`, folding inserts and deletes as needed.
	fn merge_sequential_list(&mut self, other: &OpList) {
		for op in &other.ops {
			if op.len == 0 {
				continue;
			} else if op.len > 0 {
				Self::merge_insert(&mut self.ops, *op);
			} else {
				Self::merge_delete(&mut self.ops, *op);
			}
		}
	}

	/// Merges a positive-length operation into an ordered list, combining adjacent inserts at the same base.
	fn merge_insert(ranges: &mut Vec<Op>, op: Op) {
		debug_assert!(op.len > 0);

		let mut idx = 0;
		while idx < ranges.len() && ranges[idx].ins < op.ins {
			idx += 1;
		}

		while idx < ranges.len() && ranges[idx].ins == op.ins {
			if ranges[idx].len > 0 {
				ranges[idx].len += op.len;
				return;
			}
			idx += 1;
		}

		ranges.insert(idx, op);
	}

	/// Merges a delete operation into an ordered list, coalescing overlapping delete spans.
	fn merge_delete(ranges: &mut Vec<Op>, op: Op) {
		debug_assert!(op.len < 0);

		let mut delete_start = op.ins as i64;
		let mut delete_end = Self::delete_end(&op) as i64;
		let original_len = ranges.len();
		let mut read_idx: usize = 0;
		let mut write_idx: usize = 0;
		let mut inserted = false;
		let mut inserted_idx: Option<usize> = None;

		while read_idx < original_len {
			let current = ranges[read_idx];
			read_idx += 1;

			if current.len < 0 {
				let current_start = current.ins as i64;
				let current_end = Self::delete_end(&current) as i64;

				if current_end < delete_start {
					Self::write_op(ranges, write_idx, current);
					write_idx += 1;
					continue;
				}

				if current_start > delete_end {
					if !inserted {
						let delete_op = Self::delete_span(delete_start, delete_end);
						Self::write_op(ranges, write_idx, delete_op);
						inserted_idx = Some(write_idx);
						write_idx += 1;
						inserted = true;
					}
					Self::write_op(ranges, write_idx, current);
					write_idx += 1;
					continue;
				}

				delete_start = delete_start.min(current_start);
				delete_end = delete_end.max(current_end);
				if let Some(idx) = inserted_idx {
					ranges[idx] = Self::delete_span(delete_start, delete_end);
				}
				continue;
			}

			let base = current.ins as i64;
			if !inserted && base >= delete_start {
				let delete_op = Self::delete_span(delete_start, delete_end);
				Self::write_op(ranges, write_idx, delete_op);
				inserted_idx = Some(write_idx);
				write_idx += 1;
				inserted = true;
			}

			Self::write_op(ranges, write_idx, current);
			write_idx += 1;
		}

		if !inserted {
			let delete_op = Self::delete_span(delete_start, delete_end);
			Self::write_op(ranges, write_idx, delete_op);
			inserted_idx = Some(write_idx);
			write_idx += 1;
		}

		ranges.truncate(write_idx);
	}

	/// Applies an insert to an in-progress sequential range list, respecting insertion bias.
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

	/// Applies a delete to an in-progress sequential range list by walking gaps and existing inserts.
	fn apply_delete(ranges: &mut Vec<Op>, pos: InsertPos, len: Length) {
		if len <= 0 {
			return;
		}

		let delete_start = pos as i64;
		let delete_end = delete_start + len as i64;
		let mut delete_cursor = delete_start;

		let mut doc_cursor: i64 = 0;
		let mut base_cursor: i64 = 0;
		let mut write_idx: usize = 0;
		let original_len = ranges.len();
		let mut last_delete_idx: Option<usize> = None;

		let mut read_idx = 0;
		while read_idx < original_len {
			let mut current = ranges[read_idx];
			read_idx += 1;
			let next_ins = current.ins as i64;

			if next_ins > base_cursor {
				let gap_len = next_ins - base_cursor;
				let (overlap_len, overlap_start) = Self::segment_overlap(doc_cursor, gap_len, delete_cursor, delete_end);
				if overlap_len > 0 {
					let base_offset = overlap_start - doc_cursor;
					let base_start = base_cursor + base_offset;
					Self::emit_delete_op(
						ranges,
						&mut write_idx,
						&mut last_delete_idx,
						DeleteEmit::DocSpan { base_start, len: overlap_len },
					);
					delete_cursor += overlap_len;
				}
				doc_cursor += gap_len;
				base_cursor = next_ins;
			}

			if current.len < 0 {
				base_cursor += i64::from(-current.len);
				Self::emit_delete_op(
					ranges,
					&mut write_idx,
					&mut last_delete_idx,
					DeleteEmit::Existing(current),
				);
			} else if current.len > 0 {
				let seg_len = current.len as i64;
				let (overlap_len, _) = Self::segment_overlap(doc_cursor, seg_len, delete_cursor, delete_end);
				if overlap_len > 0 {
					let overlap_i32: Length = overlap_len.try_into().expect("delete span overflow");
					current.len -= overlap_i32;
					delete_cursor += overlap_len;
				}

				if current.len > 0 {
					Self::write_op(ranges, write_idx, current);
					write_idx += 1;
				}

				doc_cursor += seg_len;
			}
		}

		if delete_cursor < delete_end {
			let seg_start = doc_cursor;
			let overlap_start = delete_cursor.max(seg_start);
			let overlap_len = delete_end - overlap_start;
			let base_offset = overlap_start - seg_start;
			let base_start = base_cursor + base_offset;
			Self::emit_delete_op(
				ranges,
				&mut write_idx,
				&mut last_delete_idx,
				DeleteEmit::DocSpan { base_start, len: overlap_len },
			);
		}

		ranges.truncate(write_idx);
	}

	/// Inserts a positive-length span at the computed index, coalescing with neighbors when possible.
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

	/// Finds where a given document position lives within the range list, honoring the provided bias.
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

	/// Writes an operation into the vector, growing it only when needed.
	fn write_op(ranges: &mut Vec<Op>, idx: usize, op: Op) {
		if idx < ranges.len() {
			ranges[idx] = op;
		} else {
			ranges.push(op);
		}
	}

	/// Returns the overlap between a segment and a delete window as `(length, start)`.
	fn segment_overlap(seg_start: i64, seg_len: i64, delete_cursor: i64, delete_end: i64) -> (i64, i64) {
		if seg_len <= 0 || delete_cursor >= delete_end {
			return (0, 0);
		}

		let seg_end = seg_start + seg_len;
		if delete_cursor >= seg_end || delete_end <= seg_start {
			return (0, 0);
		}

		let start = seg_start.max(delete_cursor);
		let end = seg_end.min(delete_end);
		(end - start, start)
	}

	/// Emits a delete operation, extending the previous delete when adjacent.
	fn emit_delete_op(
		ranges: &mut Vec<Op>,
		write_idx: &mut usize,
		last_delete_idx: &mut Option<usize>,
		source: DeleteEmit,
	) {
		let delete_op = match source {
			DeleteEmit::Existing(op) => {
				debug_assert!(op.len <= 0);
				if op.len == 0 {
					return;
				}
				op
			}
			DeleteEmit::DocSpan { base_start, len } => {
				if len <= 0 {
					return;
				}
				let ins: InsertPos = base_start.try_into().expect("delete base overflow");
				let len_i32: Length = len.try_into().expect("delete len overflow");
				Op { ins, len: -len_i32 }
			}
		};

		if let Some(idx) = *last_delete_idx {
			if Self::delete_end(&ranges[idx]) == delete_op.ins {
				ranges[idx].len += delete_op.len;
				return;
			}
		}

		Self::write_op(ranges, *write_idx, delete_op);
		*last_delete_idx = Some(*write_idx);
		*write_idx += 1;
	}

	/// Computes the exclusive end position of a delete operation.
	fn delete_end(op: &Op) -> InsertPos {
		debug_assert!(op.len < 0);
		let end = op.ins as i64 - op.len as i64;
		end as InsertPos
	}

	/// Creates a delete operation spanning from `start` to `end` in base coordinates.
	fn delete_span(start: i64, end: i64) -> Op {
		debug_assert!(end > start);
		let ins: InsertPos = start.try_into().expect("delete base overflow");
		let len_i64 = end - start;
		let len: Length = len_i64.try_into().expect("delete len overflow");
		Op { ins, len: -len }
	}
}

fn main() {
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Verifies merging sequential lists coalesce correctly for mixed insert/delete cases.
	#[test]
	fn merge_sequential_list_behaviors() {
		// Provided case: inserts combine and new positions are appended.
		let mut existing = getOpList([(5, 2), (10, 1)]);
		let additions = getOpList([(5, 3), (7, 1)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(5, 5), (7, 1), (10, 1)]);
		assert_eq!(existing, expected);

		// Provided case (also covers previous delete-span test): deletes union together.
		let mut existing = getOpList([(5, -1)]);
		let additions = getOpList([(6, -1)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(5, -2)]);
		assert_eq!(existing, expected);

		// Provided case: delete spans across multiple segments.
		let mut existing = getOpList([(3, -1), (3, 1), (6, -1)]);
		let additions = getOpList([(4, -2)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(3, -4), (3, 1)]);
		assert_eq!(existing, expected);

		// Provided case: delete must land before positive insert at same base.
		let mut existing = getOpList([(5, 1)]);
		let additions = getOpList([(5, -2)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(5, -2), (5, 1)]);
		assert_eq!(existing, expected);

		// Existing case: inserts at identical base sum their lengths.
		let mut existing = getOpList([(5, 2)]);
		let additions = getOpList([(5, 3)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(5, 5)]);
		assert_eq!(existing, expected);

		// Existing case: mixed operations keep final ordering and RLE.
		let mut existing = getOpList([(5, -2), (5, 1)]);
		let additions = getOpList([(5, 1), (6, -1)]);
		existing.merge_sequential_list(&additions);
		let expected = getOpList([(5, -2), (5, 2)]);
		assert_eq!(existing, expected);
	}

	/// Ensures sequential lists are converted back into op lists with expected coordinates.
	#[test]
	fn sequential_list_to_oplist_emits_expected_ops() {
		let mut sequential = getOpList([(5, -1), (5, 1)]);
		sequential.from_sequential_list_to_oplist();
		let expected = getOpList([(6, -1), (5, 1)]);
		assert_eq!(sequential, expected);

		let mut sequential = getOpList([(2, -4)]);
		sequential.from_sequential_list_to_oplist();
		let expected = getOpList([(6, -4)]);
		assert_eq!(sequential, expected);

		let mut sequential = getOpList([(2, -3), (2, 1)]);
		sequential.from_sequential_list_to_oplist();
		let expected = getOpList([(5, -3), (2, 1)]);
		assert_eq!(sequential, expected);

		let mut sequential = getOpList([(3, -1), (5, 2)]);
		sequential.from_sequential_list_to_oplist();
		let expected = getOpList([(4, -1), (4, 2)]);
		assert_eq!(sequential, expected);
	}

	/// Confirms round-trip conversions preserve simple states.
	#[test]
	fn sequential_list_preserves_simple_states() {
		let mut sequential = getOpList([(5, 2), (7, 1)]);
		let expected_state = sequential.clone();
		sequential.from_sequential_list_to_oplist();
		assert_eq!(sequential.from_oplist_to_sequential_list(), expected_state);

		let mut sequential = getOpList([(2, -4)]);
		let expected_state = sequential.clone();
		sequential.from_sequential_list_to_oplist();
		assert_eq!(sequential.from_oplist_to_sequential_list(), expected_state);
	}

	#[test]
	fn test_whats_already_implemented() {
		// This suite seeds a sequential state and layers additional ops on top. When `seq_state`
		// comes from `getOpList(ops).from_oplist_to_sequential_list()`, we assert:
		//   getOpListforTesting(seq_state, new_ops).from_oplist_to_sequential_list()
		//   == getOpList(ops + new_ops).from_oplist_to_sequential_list()
		//
		// Representation rules for the seeded sequential state:
		//   - Coordinates are in base-document space [0, inf); we visualize it as the digit string 123456789...
		//   - Deletes are stored as negative spans over that base space (e.g. (5, -2) removes base positions 5 and 6).
		//   - Inserts are anchored to a base position even if that base was deleted; deletes at a base index are ordered before inserts.
		//   - Adjacent deletes run-length encode into a single span.
		// The scenarios below use short digit strings to show how ops rewrite the base-backed sequence.

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

		// Basic test
		let test_vec: OpList = getOpListforTesting([(5,5)], [(7,-2)]);
		let expected_result = getOpList([(5,3)]);  // Should be "heo" at position 5
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// // Could be useful
		// // -11-2---
		// let test_vec: OpList = getOpListforTesting([(1,1),(1,1)], [(4,1)]);
		// let expected_result = getOpList([(1,1),(1,1),(2,1)]);
		// assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
	}

}
