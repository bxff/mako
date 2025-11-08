#![allow(warnings)]

mod RangeHashMap;

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

impl OpList {
	/// Helper function to initialize the new OpList with the first operation
	/// Returns a new OpList with discontinuous ranges. This transforms operations
	/// from the current document state to be expressed in base document coordinates.
	/// [0, ∞) <- ins 1 len 100, ins 2 len 12, ins 101 len 1.
	fn from_oplist_to_sequential_list(&self) -> OpList {
		let mut document = Document::new();

		if let Some(existing) = &self.test_op {
			document.apply_canonical_state(existing);
		}

		for op in &self.ops {
			if op.len == 0 {
				continue;
			}

			if op.len.is_positive() {
				document.insert_at_doc(i64::from(op.ins), i64::from(op.len));
			} else {
				let start = i64::from(op.ins) + i64::from(op.len);
				let end = i64::from(op.ins);
				assert!(start >= 0 && end >= 0, "Delete operations cannot target negative positions");
				document.delete_doc_range(start, end);
			}
		}

		document.into_oplist()
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
	fn clean_delete(&mut self) {
		for op in self.ops.iter_mut() {
			if op.len < 0 {
				let ins = op.ins;
				op.ins = op.ins + op.len;
				op.len = -ins;
			}
		}
	}
	
	/// Human readable output for testing.
	fn clean_output(&mut self) {
		todo!(); // maybe not worthwhile
	}
}

const MAX_BASE_LEN: i64 = i32::MAX as i64;

struct Document {
	segments: Vec<Segment>,
}

#[derive(Clone)]
struct Segment {
	kind: SegmentKind,
	len: i64,
}

#[derive(Clone, Debug)]
enum SegmentKind {
	Base { start: i64 },
	Insert { anchor: i64 },
}

impl Document {
	fn new() -> Self {
		Self {
			segments: vec![Segment {
				kind: SegmentKind::Base { start: 0 },
				len: MAX_BASE_LEN,
			}],
		}
	}

	fn apply_canonical_state(&mut self, ops: &[(InsertPos, Length)]) {
		if ops.is_empty() {
			return;
		}

		for &(ins, len) in ops.iter().filter(|(_, len)| *len < 0) {
			self.delete_base_range(i64::from(ins), i64::from(-len));
		}

		for &(ins, len) in ops.iter().filter(|(_, len)| *len > 0) {
			self.insert_at_base(i64::from(ins), i64::from(len));
		}
	}

	fn insert_at_doc(&mut self, doc_pos: i64, len: i64) {
		if len == 0 {
			return;
		}

		let (idx, anchor) = self.ensure_boundary(doc_pos);
		self.segments.insert(idx, Segment {
			kind: SegmentKind::Insert { anchor },
			len,
		});
		self.merge_neighbors(idx);
	}

	fn delete_doc_range(&mut self, start: i64, end: i64) {
		if start >= end {
			return;
		}

		let (start_idx, _) = self.ensure_boundary(start);
		let (end_idx, _) = self.ensure_boundary(end);
		self.segments.drain(start_idx..end_idx);

		if start_idx > 0 {
			self.try_merge_pair(start_idx - 1);
		}
		if start_idx < self.segments.len() {
			self.try_merge_pair(start_idx);
		}
	}

	fn insert_at_base(&mut self, anchor: i64, len: i64) {
		if len == 0 {
			return;
		}
		let doc_pos = self.doc_pos_for_base(anchor);
		let (idx, _) = self.ensure_boundary(doc_pos);
		self.segments.insert(idx, Segment {
			kind: SegmentKind::Insert { anchor },
			len,
		});
		self.merge_neighbors(idx);
	}

	fn delete_base_range(&mut self, base_start: i64, len: i64) {
		if len == 0 {
			return;
		}
		let doc_start = self.doc_pos_for_base(base_start);
		let doc_end = self.doc_pos_for_base(base_start + len);
		self.delete_doc_range(doc_start, doc_end);
	}

	fn ensure_boundary(&mut self, doc_pos: i64) -> (usize, i64) {
		assert!(doc_pos >= 0, "Document positions cannot be negative");

		let mut cursor = 0i64;
		let mut anchor_at_cursor = 0i64;

		for idx in 0..self.segments.len() {
			let seg_len = self.segments[idx].len;
			if doc_pos == cursor {
				return (idx, anchor_at_cursor);
			}

			if doc_pos < cursor + seg_len {
				let offset = doc_pos - cursor;
				let anchor = match &self.segments[idx].kind {
					SegmentKind::Base { start } => *start + offset,
					SegmentKind::Insert { anchor } => *anchor,
				};

				if offset == 0 {
					return (idx, anchor);
				}

				let mut right = self.segments[idx].clone();
				right.len = seg_len - offset;
				match &mut right.kind {
					SegmentKind::Base { start } => *start += offset,
					SegmentKind::Insert { .. } => {}
				}
				self.segments[idx].len = offset;
				self.segments.insert(idx + 1, right);
				return (idx + 1, anchor);
			}

			cursor += seg_len;
			anchor_at_cursor = match &self.segments[idx].kind {
				SegmentKind::Base { start } => *start + seg_len,
				SegmentKind::Insert { anchor } => *anchor,
			};
		}

		(self.segments.len(), anchor_at_cursor)
	}

	fn doc_pos_for_base(&self, base_pos: i64) -> i64 {
		let mut doc_cursor = 0i64;

		for seg in &self.segments {
			match &seg.kind {
				SegmentKind::Base { start } => {
					if base_pos < *start {
						return doc_cursor;
					}

					let end = *start + seg.len;
					if base_pos <= end {
						return doc_cursor + (base_pos - *start);
					}

					doc_cursor += seg.len;
				}
				SegmentKind::Insert { anchor } => {
					if base_pos == *anchor {
						return doc_cursor;
					}
					doc_cursor += seg.len;
				}
			}
		}

		doc_cursor
	}

	fn merge_neighbors(&mut self, idx: usize) {
		if idx > 0 {
			self.try_merge_pair(idx - 1);
		}
		self.try_merge_pair(idx);
	}

	fn try_merge_pair(&mut self, left: usize) {
		if left + 1 >= self.segments.len() {
			return;
		}

		let (can_merge, anchor) = match (&self.segments[left].kind, &self.segments[left + 1].kind) {
			(SegmentKind::Base { start: left_start }, SegmentKind::Base { start: right_start }) => {
				let expected = *left_start + self.segments[left].len;
				(expected == *right_start, *left_start)
			}
			(
				SegmentKind::Insert { anchor: left_anchor },
				SegmentKind::Insert { anchor: right_anchor },
			) => (*left_anchor == *right_anchor, *left_anchor),
			_ => (false, 0),
		};

		if !can_merge {
			return;
		}

		self.segments[left].len += self.segments[left + 1].len;
		if let SegmentKind::Base { start } = &mut self.segments[left].kind {
			*start = anchor;
		}
		self.segments.remove(left + 1);
	}

	fn into_oplist(self) -> OpList {
		println!("=== DEBUG: into_oplist() called ===");
		println!("Document segments before transformation:");
		for (i, seg) in self.segments.iter().enumerate() {
			println!("  [{}]: {:?} (len: {})", i, seg.kind, seg.len);
		}
		let deletes = self.collect_delete_ops();
		let inserts = self.collect_insert_ops();
		println!("Collected deletes: {:?}", deletes);
		println!("Collected inserts: {:?}", inserts);
		let mut combined = Vec::with_capacity(deletes.len() + inserts.len());

		for op in deletes {
			combined.push((op, OrderKey::Delete));
		}

		for (op, order) in inserts {
			combined.push((op, OrderKey::Insert(order)));
		}

		combined.sort_by(|(left, left_kind), (right, right_kind)| {
			match left.ins.cmp(&right.ins) {
				std::cmp::Ordering::Equal => match (left_kind, right_kind) {
					(OrderKey::Insert(a), OrderKey::Insert(b)) => a.cmp(b),
					(OrderKey::Insert(_), OrderKey::Delete) => std::cmp::Ordering::Less,
					(OrderKey::Delete, OrderKey::Insert(_)) => std::cmp::Ordering::Greater,
					(OrderKey::Delete, OrderKey::Delete) => std::cmp::Ordering::Equal,
				},
				other => other,
			}
		});

		println!("Combined operations before sorting: {:?}", combined);
		combined.sort_by(|(left, left_kind), (right, right_kind)| {
			match left.ins.cmp(&right.ins) {
				std::cmp::Ordering::Equal => match (left_kind, right_kind) {
					(OrderKey::Insert(a), OrderKey::Insert(b)) => a.cmp(b),
					(OrderKey::Insert(_), OrderKey::Delete) => std::cmp::Ordering::Less,
					(OrderKey::Delete, OrderKey::Insert(_)) => std::cmp::Ordering::Greater,
					(OrderKey::Delete, OrderKey::Delete) => std::cmp::Ordering::Equal,
				},
				other => other,
			}
		});
		println!("Combined operations after sorting: {:?}", combined);
		let ops = combined.into_iter().map(|(op, _)| op).collect();
		let result = OpList { ops, test_op: None };
		println!("Final OpList: {:?}", result);
		println!("=== END DEBUG into_oplist() ===");
		result
	}

	fn collect_delete_ops(&self) -> Vec<Op> {
		let mut deletes = Vec::new();
		let mut expected_start = 0i64;

		for seg in &self.segments {
			if let SegmentKind::Base { start } = &seg.kind {
				if *start > expected_start {
					let len = *start - expected_start;
					deletes.push(Op {
						ins: to_insert_pos(expected_start),
						len: -to_length(len),
					});
				}
				expected_start = *start + seg.len;
				if expected_start >= MAX_BASE_LEN {
					break;
				}
			}
		}

		deletes
	}

	fn collect_insert_ops(&self) -> Vec<(Op, usize)> {
		let mut inserts: Vec<(Op, usize)> = Vec::new();
		let mut order = 0usize;

		for seg in &self.segments {
			if let SegmentKind::Insert { anchor } = &seg.kind {
				if seg.len == 0 {
					continue;
				}

				if let Some((last_op, _)) = inserts.last_mut() {
					if last_op.ins == to_insert_pos(*anchor) {
						last_op.len += to_length(seg.len);
						continue;
					}
				}

				inserts.push((
					Op {
						ins: to_insert_pos(*anchor),
						len: to_length(seg.len),
					},
					order,
				));
				order += 1;
			}
		}

		inserts
	}
}

#[derive(Debug)]
enum OrderKey {
	Insert(usize),
	Delete,
}

fn to_length(value: i64) -> Length {
	i32::try_from(value).expect("Length exceeded i32 range")
}

fn to_insert_pos(value: i64) -> InsertPos {
	i32::try_from(value).expect("Insert position exceeded i32 range")
}

/// Enumeration for control flow in loops
#[derive(Debug, PartialEq)]
enum BreakOrContinue {
    Break,
    Continue,
}

/// Configuration for random test data generation
#[derive(Debug)]
struct TestDataConfig {
    pub num_tests: usize,
    pub min_insert_pos: InsertPos,
    pub max_insert_pos: InsertPos,
    pub min_length: Length,
    pub max_length: Length,
}

impl Default for TestDataConfig {
    fn default() -> Self {
        Self {
            num_tests: 10,
            min_insert_pos: 1,
            max_insert_pos: 10,
            min_length: -5,
            max_length: 5,
        }
    }
}

/// Generates random test data for testing operations
/// Note: This is not implemented correctly yet, specifically inclusive range is random.
/// This is very poorly optimized but shouldn't matter for testing.
fn generate_random_test_data(num_tests: usize) -> OpList {
	generate_random_test_data_with_config(TestDataConfig {
		num_tests,
		..Default::default()
	})
}

/// Generates random test data with custom configuration
fn generate_random_test_data_with_config(config: TestDataConfig) -> OpList {
	let mut test_data = Vec::with_capacity(config.num_tests);
	let mut rng = thread_rng();

	for _ in 0..config.num_tests {
		let ins: InsertPos = rng.gen_range(config.min_insert_pos..=config.max_insert_pos);
		let len: Length = rng.gen_range(config.min_length..=config.max_length);
		
		// Skip invalid operations
		if len == 0 || (len + ins) < 0 {
			continue;
		}
		
		test_data.push(Op { ins, len });
	}

	OpList { ops: test_data, test_op: None }
}

/// Old code: generating only positive random test data for testing. 
fn generate_only_positive_random_test_data(num_tests: usize) -> OpList {
	let mut test_data = Vec::with_capacity(num_tests);
	let mut rng = thread_rng();

	for _ in 0..num_tests {
		let ins = rng.gen_range(1..=10);
		let len = rng.gen_range(1..=10);
		test_data.push(Op { ins, len });
	}

	OpList { ops: test_data, test_op: None }
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
