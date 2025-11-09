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

impl OpList {
	/// Helper function to initialize the new OpList with the first operation
	fn initialize_new_oplist(&self) -> (OpList, usize) {
		// Create empty OpList to be populated
		let mut new_oplist = OpList { ops: Vec::new(), test_op: None };
		
		// Determine starting index based on three conditions
		let start_index = if let Some(test_data) = &self.test_op {
			// Condition 1: Use test data if available
			new_oplist = getOpListbyVec(test_data.clone());
			0 // Start from beginning since we're using all test data
		} else if self.ops.is_empty() {
			// Condition 2: No operations to process
			0 // Start from beginning with empty list
		} else {
			// Condition 3: Process first operation normally
			let first_op = self.ops[0];
			let initial_op = if first_op.len.is_negative() {
				// For delete operations: normalize position by adding length
				// Example: ins=5, len=-2 becomes ins=3, len=-2
				Op { 
					ins: first_op.ins + first_op.len, 
					len: first_op.len 
				}
			} else {
				// For insert operations: keep original position
				Op { 
					ins: first_op.ins, 
					len: first_op.len 
				}
			};
			new_oplist.ops.push(initial_op);
			1 // Start from second operation since first is processed
		};
		(new_oplist, start_index)
	}

	/// Helper function to normalize delete operation position
	fn normalize_delete_op(op: &Op) -> Op {
		if op.len.is_negative() {
			Op { 
				ins: op.ins + op.len, 
				len: op.len 
			}
		} else {
			*op
		}
	}

	/// Helper function to check if an operation is within a range
	fn is_op_in_range(op_ins: InsertPos, start_range: InsertPos, end_range: InsertPos) -> bool {
		op_ins >= start_range && op_ins <= end_range
	}

	/// Helper function to remove zero-length ranges
	/// More efficient than removing one by one as it uses retain
	fn remove_zero_length_ranges(ops: &mut Vec<Op>, start_index: usize) {
		if start_index >= ops.len() {
			return;
		}

		// Split the vector and only process the part from start_index
		let mut temp_ops = ops.split_off(start_index);
		temp_ops.retain(|op| op.len != 0);
		ops.append(&mut temp_ops);
	}

	/// Helper function to handle the last element logic that was repeated in multiple places
	/// This reduces code duplication for the `if i == new_oplist_len - 1` blocks
	fn handle_last_element_logic(
		op_ins: InsertPos,
		op_len: Length,
		range: &Op,
		aggregate_len: Length,
		original_doc_delete_range: &mut Op,
		range_delete_index: &mut usize,
		new_range: &mut Op,
		range_insertion_index: &mut usize,
		i: usize,
	) {
		if op_len.is_positive() {
			*new_range = Op {
				ins: op_ins - aggregate_len - range.len,
				len: op_len
			};
			*range_insertion_index = i + 1;
		} else {
			if original_doc_delete_range.ins == i32::MAX {
				original_doc_delete_range.ins = op_ins - range.len - aggregate_len;
				original_doc_delete_range.len = (op_ins - op_len - range.len - aggregate_len) - (op_ins - range.len - aggregate_len);
				*range_delete_index = i + 1;
			} else {
				original_doc_delete_range.len += (op_ins - op_len - range.len - aggregate_len) - (op_ins - range.len - aggregate_len);
			}
		}
	}

	/// Returns a new OpList with discontinuous ranges. This transforms operations
	/// from the current document state to be expressed in base document coordinates.
	/// [0, ∞) <- ins 1 len 100, ins 2 len 12, ins 101 len 1.
	fn from_oplist_to_sequential_list(&self) -> OpList {
		// Initialize the new OpList with existing state
		let (mut new_oplist, start_from_for_ops) = self.initialize_new_oplist();

		// Process each operation from the starting point
		for op in self.ops[start_from_for_ops..].iter() {
			// Normalize delete operation position for base coordinates
			let normalized_op = Self::normalize_delete_op(op);
			let mut op_ins = normalized_op.ins;
			let mut op_len = normalized_op.len;

			// === STATE TRACKING VARIABLES ===
			// These variables track the processing state for each operation
			let mut aggregate_len: Length = 0;              // Cumulative length adjustment from processed ranges
			let mut range_insertion_index: usize = usize::MAX;  // Where to insert new insert operations
			let mut range_delete_index: usize = usize::MAX;     // Where to insert new delete operations
			let mut start_range: InsertPos;                   // Start boundary of current range in base coords
			let mut end_range: InsertPos = i32::MAX;          // End boundary of current range in base coords

			// === OPERATION TRACKING ===
			let mut new_range = Op { ins: i32::MAX, len: i32::MAX };           // New operation to insert
			let mut original_doc_delete_range = Op { ins: i32::MAX, len: i32::MAX }; // Delete operation in base coords
			let mut last_op_to_be_delete = false;             // Flag for incomplete delete processing
			let mut to_delete_zero_ranges_from = usize::MAX;  // Starting index for zero-length range cleanup

			// === DELETE RANGE TRACKING ===
			// Track previous delete ranges for complex overlap scenarios
			let mut previous_delete_range_start: InsertPos = i32::MAX;
			let mut previous_delete_range_end: InsertPos = i32::MAX;
			let mut last_delete_range_start: InsertPos = i32::MAX;
			let mut last_delete_range_end: InsertPos = i32::MAX;
			let mut last_delete_range_index: usize = usize::MAX;

			let new_oplist_len = new_oplist.ops.len();

			// === MAIN RANGE PROCESSING LOOP ===
			// Iterate through each existing range to determine where the new operation fits
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				last_op_to_be_delete = false;

				// === HANDLE DELETE RANGES (negative length) ===
				if range.len.is_negative() {
					// Track current delete range boundaries
					previous_delete_range_start = range.ins;
					previous_delete_range_end = previous_delete_range_start + (-range.len);

					// Calculate range boundaries in base coordinates
					start_range = if end_range != i32::MAX {
						end_range  // Use previous end as current start
					} else {
						0          // Start from beginning for first range
					};

					// Validate delete range position
					if (range.ins as Length + aggregate_len) > 0 {
						end_range = range.ins as Length + aggregate_len;
					} else {
						panic!("Deletes should delete into the negative range, e.g. (4,-5) shouldn't exist.");
					}

					// Check if current operation falls within this delete range
					if Self::is_op_in_range(op_ins, start_range, end_range) {
						if op_len.is_positive() {
							// === INSERT OPERATION WITHIN DELETE RANGE ===
							new_range = Op {
								ins: op_ins - aggregate_len,  // Convert to base coordinates
								len: op_len
							};
							range_insertion_index = i;
							break;  // Operation placed, stop processing
						} else {
							// === DELETE OPERATION WITHIN DELETE RANGE ===
							// Complex logic for delete-over-delete scenarios
							if op_ins - op_len > end_range && (op_ins != end_range) {
								// Delete extends beyond current range end
								if original_doc_delete_range.ins == i32::MAX {
									original_doc_delete_range.ins = op_ins - aggregate_len;
									original_doc_delete_range.len = end_range - aggregate_len - original_doc_delete_range.ins;
									range_delete_index = i;

									original_doc_delete_range.len -= range.len;
									aggregate_len += range.len;
									range.len = 0;  // Mark range for removal
									if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
										to_delete_zero_ranges_from = i;
									}

									// Adjust remaining operation
									op_len = op_ins - op_len - end_range;
									op_ins = end_range;
									op_len = -op_len;
									last_op_to_be_delete = true;
								} else {
									original_doc_delete_range.len += (end_range - aggregate_len) - (op_ins - aggregate_len);
									op_len = op_ins - op_len - end_range;
									op_ins = end_range;
									op_len = -op_len;
									last_op_to_be_delete = true;
								}
							} else if op_ins != end_range {
								// Delete doesn't extend beyond range end
								if original_doc_delete_range.ins == i32::MAX {
									original_doc_delete_range.ins = op_ins - aggregate_len;
									original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
									range_delete_index = i;
									break;
								} else {
									original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
									break;
								}
							} else if i == new_oplist_len - 1 {
								// Last element special case
								Self::handle_last_element_logic(
									op_ins, op_len, range, aggregate_len,
									&mut original_doc_delete_range, &mut range_delete_index,
									&mut new_range, &mut range_insertion_index, i
								);
							}
						}
					} else if i == new_oplist_len - 1 {
						// Operation is after this range (last range case)
						Self::handle_last_element_logic(
							op_ins, op_len, range, aggregate_len,
							&mut original_doc_delete_range, &mut range_delete_index,
							&mut new_range, &mut range_insertion_index, i
						);
					}

					// Update delete range tracking for next iteration
					last_delete_range_start = range.ins;
					last_delete_range_end = last_delete_range_start + (-range.len);
					last_delete_range_index = i;
					aggregate_len += range.len;
					continue;  // Move to next range
				}

				// === HANDLE INSERT RANGES (positive length) ===
				// Adjust start position for ranges affected by previous deletes
				if range.ins > previous_delete_range_start && range.ins <= previous_delete_range_end {
					start_range = previous_delete_range_start +
						(previous_delete_range_end - previous_delete_range_start) + aggregate_len;
				} else {
					start_range = range.ins + aggregate_len;
				}
				end_range = start_range + range.len;

				// Check if current operation falls within this insert range
				if Self::is_op_in_range(op_ins, start_range, end_range) {
					if op_len.is_positive() {
						// === INSERT OPERATION WITHIN INSERT RANGE ===
						// Extend existing range (run-length encoding)
						range.len += op_len;
						range_insertion_index = usize::MAX;  // Mark as merged
						break;
					} else {
						// === DELETE OPERATION WITHIN INSERT RANGE ===
						if op_ins - op_len - aggregate_len > (end_range - aggregate_len) && (op_ins != end_range) {
							// Delete extends beyond range end
							range.len -= (end_range - aggregate_len) - (op_ins - aggregate_len);
							if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
								to_delete_zero_ranges_from = i;
							}

							aggregate_len += (end_range - aggregate_len) - (op_ins - aggregate_len);

							op_len = op_ins - op_len - end_range;
							op_ins = end_range;
							op_len = -op_len;
							last_op_to_be_delete = true;
						} else if op_ins != end_range {
							// Delete doesn't extend beyond range end
							range.len -= op_ins - op_len - aggregate_len - (op_ins - aggregate_len);
							if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
								to_delete_zero_ranges_from = i;
							}
							range_insertion_index = usize::MAX;
							break;
						} else if i == new_oplist_len - 1 {
							Self::handle_last_element_logic(
								op_ins, op_len, range, aggregate_len,
								&mut original_doc_delete_range, &mut range_delete_index,
								&mut new_range, &mut range_insertion_index, i
							);
						}
					}
				} else if op_ins < start_range {
					// === OPERATION BEFORE CURRENT RANGE ===
					if op_len.is_positive() {
						// Insert before this range
						new_range = Op {
							ins: op_ins - aggregate_len,
							len: op_len
						};
						range_insertion_index = i;
						break;
					} else {
						// === DELETE BEFORE CURRENT RANGE ===
						// Complex overlap logic for delete spanning multiple ranges
						if op_ins - op_len - aggregate_len > (start_range - aggregate_len) &&
						   op_ins - op_len - aggregate_len <= (end_range - aggregate_len) {
							// Delete overlaps with range start
							if original_doc_delete_range.ins == i32::MAX {
								original_doc_delete_range.ins = op_ins - aggregate_len;
								original_doc_delete_range.len = (start_range - aggregate_len) - original_doc_delete_range.ins;

								range.len -= (op_ins - op_len - aggregate_len) - (start_range - aggregate_len);
								if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
									to_delete_zero_ranges_from = i;
								}
								range_delete_index = i;
								break;
							} else {
								original_doc_delete_range.len += (start_range - aggregate_len) - (op_ins - aggregate_len);
								range.len -= (op_ins - op_len - aggregate_len) - (start_range - aggregate_len);
								if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
									to_delete_zero_ranges_from = i;
								}
								break;
							}
						} else if op_ins - op_len - aggregate_len > end_range - aggregate_len {
							// Delete extends beyond range entirely
							if original_doc_delete_range.ins == i32::MAX {
								original_doc_delete_range.ins = op_ins - aggregate_len;
								original_doc_delete_range.len = (start_range - aggregate_len) - original_doc_delete_range.ins;
								range_delete_index = i;

								range.len = 0;  // Mark entire range for deletion
								if to_delete_zero_ranges_from == usize::MAX {
									to_delete_zero_ranges_from = i;
								}
								op_len = op_ins - op_len - end_range;
								op_ins = end_range;
								op_len = -op_len;
								last_op_to_be_delete = true;
							} else {
								original_doc_delete_range.len += (start_range - aggregate_len) - (op_ins - aggregate_len);

								range.len = 0;
								if to_delete_zero_ranges_from == usize::MAX {
									to_delete_zero_ranges_from = i;
								}

								op_len = op_ins - op_len - end_range;
								op_ins = end_range;
								op_len = -op_len;
								last_op_to_be_delete = true;
							}
						} else {
							// Delete ends before range start
							if original_doc_delete_range.ins == i32::MAX {
								original_doc_delete_range.ins = op_ins - aggregate_len;
								original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);

								range_delete_index = i;
								break;
							} else {
								original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - op_ins - aggregate_len;

								break;
							}
						}
					}
				} else if i == new_oplist_len - 1 {
					// === OPERATION AFTER ALL RANGES ===
					Self::handle_last_element_logic(
						op_ins, op_len, range, aggregate_len,
						&mut original_doc_delete_range, &mut range_delete_index,
						&mut new_range, &mut range_insertion_index, i
					);
				}

				aggregate_len += range.len;
			}

			// === POST-PROCESSING ===

			// Handle any remaining delete operation that couldn't be processed
			if last_op_to_be_delete {
				if original_doc_delete_range.ins == i32::MAX {
					original_doc_delete_range.ins = op_ins - aggregate_len;
					original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
					range_delete_index = new_oplist.ops.len();
				} else {
					original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
				}
			}

			// Insert new range if needed
			if range_insertion_index != usize::MAX {
				debug_assert!(new_range != Op { ins: i32::MAX, len: i32::MAX });
				new_oplist.ops.insert(range_insertion_index, new_range);
			}

			// Handle delete range insertion with complex merging logic
			if range_delete_index != usize::MAX {
				debug_assert!(original_doc_delete_range != Op { ins: i32::MAX, len: i32::MAX });

				// Check if delete extends previous delete (merge adjacent deletes)
				if last_delete_range_end == original_doc_delete_range.ins {
					new_oplist.ops[last_delete_range_index].len -= original_doc_delete_range.len;

					// Check for adjacent ranges that can be merged
					if range_delete_index < new_oplist.ops.len() {
						if new_oplist.ops[range_delete_index].len.is_negative() &&
						   new_oplist.ops[range_delete_index].ins == original_doc_delete_range.ins + original_doc_delete_range.len {
							new_oplist.ops[last_delete_range_index].len += new_oplist.ops[range_delete_index].len;
							new_oplist.ops.remove(range_delete_index);
						}
					} else if range_delete_index + 1 < new_oplist.ops.len() {
						if new_oplist.ops[range_delete_index + 1].len.is_negative() &&
						   new_oplist.ops[range_delete_index + 1].ins == original_doc_delete_range.ins + original_doc_delete_range.len {
							new_oplist.ops[last_delete_range_index].len += new_oplist.ops[range_delete_index + 1].len;
							new_oplist.ops.remove(range_delete_index + 1);
						}
					}
				} else if range_delete_index < new_oplist.ops.len() {
					// Handle insertion at various positions with merge logic
					if new_oplist.ops[range_delete_index].len.is_negative() &&
					   new_oplist.ops[range_delete_index].ins == original_doc_delete_range.ins + original_doc_delete_range.len {
						new_oplist.ops[range_delete_index].ins -= original_doc_delete_range.len;
						new_oplist.ops[range_delete_index].len -= original_doc_delete_range.len;
					} else if range_delete_index + 1 < new_oplist.ops.len() {
						if new_oplist.ops[range_delete_index + 1].len.is_negative() &&
						   new_oplist.ops[range_delete_index + 1].ins == original_doc_delete_range.ins + original_doc_delete_range.len {
							new_oplist.ops[range_delete_index + 1].ins -= original_doc_delete_range.len;
							new_oplist.ops[range_delete_index + 1].len -= original_doc_delete_range.len;
						} else {
							original_doc_delete_range.len *= -1;
							new_oplist.ops.insert(range_delete_index, original_doc_delete_range);
						}
					} else {
						original_doc_delete_range.len *= -1;
						new_oplist.ops.insert(range_delete_index, original_doc_delete_range);
					}
				} else {
					original_doc_delete_range.len *= -1;
					new_oplist.ops.insert(range_delete_index, original_doc_delete_range);
				}
			}

			// Clean up zero-length ranges efficiently
			if to_delete_zero_ranges_from != usize::MAX {
				Self::remove_zero_length_ranges(&mut new_oplist.ops, to_delete_zero_ranges_from);
			}
		}

		new_oplist
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