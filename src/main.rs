mod RangeHashMap;

use std::{i128::MAX, i32};

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
		let mut start_from_for_ops: usize = 0;
		if self.test_op.is_some() {
			new_oplist = getOpListbyVec(self.test_op.clone().unwrap());
		} else {
			if self.ops[0].len.is_negative() {
				new_oplist = OpList { ops: vec![
					Op { ins: self.ops[0].ins + self.ops[0].len, len: self.ops[0].len },
				],test_op: None }; // Start with the first element in the list with the length to be zero
			} else {
				new_oplist = OpList { ops: vec![
					Op { ins: self.ops[0].ins, len: self.ops[0].len },
				],test_op: None }; // Start with the first element in the list with the length to be zero
			}
			start_from_for_ops = 1;
			// let mut new_oplist = getOpList([(1,1),(2,1),(4,-1),(5,1)]);
			// let mut new_oplist = getOpList([(5,-2),(6,1),(7,1)]);
		}

		// let mut test_vec: OpList = getOpList([(1, 1), (3, 1), (1, 1), (5, 1)]);
		// let expected_result = getOpList([(1, 2), (2, 1), (3, 1)]);
		for op in self.ops[start_from_for_ops..].iter() {
			// Current Iins
			let mut op_ins = op.ins;
			let mut op_len = op.len;

			if op_len.is_negative() {
				op_ins = op_ins + op_len; // convert 5,-2 to 3,-2 for comparing ins with start and end range
			}

			let mut aggerate_len: i32 = 0;

			let mut range_insertion_index: usize = usize::MAX;
			let mut range_delete_index: usize = usize::MAX;

			let mut start_range: i32;
			let mut end_range: i32 = i32::MAX;

			let mut new_range = Op { ins: i32::MAX, len: i32::MAX };
			let mut orignal_doc_delete_range = Op { ins: i32::MAX, len: 0 }; // basically this delete keeps track of original document deletes which are being extended.
			let mut orignal_doc_delete_range = Op { ins: i32::MAX, len: i32::MAX }; // basically this delete keeps track of original document deletes which are being extended.
			// deletes are stored basically as start,len where len is not negative
			let mut last_op_to_be_delete = false;
			// start finding 0 len ranges and delete them from the list from this point to the end of the list.
			let mut to_delete_zero_ranges_from = usize::MAX;

			// required for fixing elements which are already deleted in the orignal and are extended.
			let mut previous_delete_range_start: i32 = i32::MAX;
			let mut previous_delete_range_end: i32 = i32::MAX;

			let mut last_delete_range_start: i32 = i32::MAX;
			let mut last_delete_range_end: i32 = i32::MAX;
			let mut last_delete_range_index: usize = usize::MAX;

			let mut range_len:i32 = i32::MAX;
			let ops_len = new_oplist.ops.len();
			for (i, range) in new_oplist.ops.iter_mut().enumerate() {
				dbg!(op_ins);
				dbg!(op_len);
				last_op_to_be_delete = false;
				if range.len.is_negative() { // if we itering a negative range, we just want to check for splitting 

					// range_len = range.len;
					// This block is basically checking for split insertions just before a negative range.
					// it can't really extend, more like insert in between or extend the delete.


					// 7,-2 -> 7,9
					previous_delete_range_start = range.ins;
					previous_delete_range_end = previous_delete_range_start + (-range.len);


					if end_range != i32::MAX {
						start_range = end_range; // previous end range, we need this as we need previous op.ins to find ending.
					} else { start_range = 0 }
					if (range.ins as i32 + aggerate_len) > 0 { // When is this not true? Delete start + agg_len > 0 means that delete range is going back.
						end_range = range.ins as i32 + aggerate_len; // so for (4,-1) we are considering 4 as the end, i.g. agg_len + 4.
					} else {
						panic!("Deletes should delete into the negative range, e.g. (4,-5) shouldn't exists.");
						// end_range = range.ins 
					}
					dbg!(start_range);
					dbg!(end_range);

					// op_ins, start_range, etc are trasformed to represent current document start and end and not original document.
					// there of those - aggerate_len simply gives trasformed document based on the original document
					if op_ins >= start_range && op_ins <= end_range { // Split new insertion in the sequential list.
						if op_len.is_positive() {
							new_range = Op {
								ins: op_ins - aggerate_len,
								len: op_len
							};
							range_insertion_index = i; // If we are inserting within a deleted range, we would want the insert to go after the delete; nah its afterwords would be handled ahead
							break;	
						} else {
							if op_ins - op_len > end_range && (op_ins != end_range) { // delete extends beyond end of range, therefor we continue to find deletes.
								if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
									orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
									orignal_doc_delete_range.len = end_range - aggerate_len - orignal_doc_delete_range.ins; // end range - delete start
									range_delete_index = i;

									// As delete is extending beyond another delete, we add that delete on this list and queue the delete range to be deleted.
									orignal_doc_delete_range.len -= range.len;
									aggerate_len += range.len;
									range.len = 0;
									if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
										to_delete_zero_ranges_from = i;
									}
									
									op_len = op_ins - op_len - (end_range); // delete end - end range
									op_ins = end_range; // end range
									op_len = -op_len;
									last_op_to_be_delete = true;
									dbg!(orignal_doc_delete_range);
									dbg!(op_ins);
									dbg!(op_len);
								} else { // If delete_range already exists
									orignal_doc_delete_range.len += (end_range - aggerate_len) - (op_ins - aggerate_len); // end range - delete start
									op_len = op_ins - op_len - (end_range); // delete end - end range
									op_ins = end_range;
									op_len = -op_len;
									last_op_to_be_delete = true;
								}
							} else if op_ins != end_range { // delete doesn't extends beyond end of range
								if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
									orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
									orignal_doc_delete_range.len = (op_ins - op_len - aggerate_len) - (op_ins - aggerate_len); // delete end - delete start
									range_delete_index = i;
									break;	
								} else {
									orignal_doc_delete_range.len += (op_ins - op_len - aggerate_len) - (op_ins - aggerate_len); // delete end - delete start
									break;	
								}
							} else if i == ops_len - 1 {
								if op_len.is_positive() {
									new_range = Op {
										ins: op_ins - aggerate_len - range.len, // basically if we are inserting after the range, we would also want the range len included in aggerate len
										len: op_len
									};
									range_insertion_index = i+1; // Just insert after the last range
								} else { // basically it cannot further extend so we are using the case of it doesn't extend beyond end of range
		
									// start from here, the problem is that we are deleting, so we need to take this as last messure.
		
									// ok ok lets start from after the test we just don't have much time.
								
									if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
										orignal_doc_delete_range.ins = op_ins - range.len - aggerate_len; // delete start
										orignal_doc_delete_range.len = (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
										range_delete_index = i+1;
									} else {
										orignal_doc_delete_range.len += (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
									}
								}
							}
						}
					} else if i == ops_len - 1 { // if op_ins > start_range && op_ins < end_range i.g. op is after the range
						if op_len.is_positive() {
							new_range = Op {
								ins: op_ins - aggerate_len - range.len, // basically if we are inserting after the range, we would also want the range len included in aggerate len
								len: op_len
							};
							range_insertion_index = i+1; // Just insert after the last range
						} else { // basically it cannot further extend so we are using the case of it doesn't extend beyond end of range

							// start from here, the problem is that we are deleting, so we need to take this as last messure.

							// ok ok lets start from after the test we just don't have much time.
						
							if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
								orignal_doc_delete_range.ins = op_ins - range.len - aggerate_len; // delete start
								orignal_doc_delete_range.len = (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
								range_delete_index = i+1;
							} else {
								orignal_doc_delete_range.len += (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
							}
						}
					}

					// 7,-2 -> 7,9
					last_delete_range_start = range.ins;
					last_delete_range_end = last_delete_range_start + (-range.len);
					last_delete_range_index = i;
					aggerate_len += range.len;
					continue;
				}

				// Insertions which extend deleted elements, i.g. [0,inf) is extended by (1,1) and (2,1) but [1,2] is deleted on the original list.
				// if range.ins > /* Not >= as it would always be more */ previous_delete_range_ins && range.ins <= (previous_delete_range_ins + (-previous_delete_range_len as u32)) {
				if range.ins > previous_delete_range_start && range.ins <= previous_delete_range_end {

					// This block basically changes start positions of ranges whose original length has been deleted.
					
					// 7,-2 -> 7 + 2 + (agg_len-2)
					// [(5,-2),(6,len),(7,len)]
					start_range = previous_delete_range_start + (previous_delete_range_end - previous_delete_range_start) + aggerate_len;
				} else { start_range = range.ins + aggerate_len; } // considering it as postive for positive ranges for now
				end_range = start_range + range.len; // considering it as postive for positive ranges for now

				// Possibly could proof check here?
				dbg!(start_range);
				dbg!(end_range);
				
				if op_ins >= start_range && op_ins <= end_range { // Adds to the range in the sequential list.
					// TODO: This is changed, may be wrong test it out.
					if op_len.is_positive() {
						range.len += op_len;
						range_insertion_index = usize::MAX;
						break;
					} else {
						// op_ins == end_range checks for extending, we don't want to extend deletes but we want to extend inserts
						if op_ins - op_len - aggerate_len > (end_range - aggerate_len) && (op_ins != end_range) { // delete extends beyond end of range, therefor we continue to find deletes.
							range.len -= (end_range - aggerate_len) - (op_ins - aggerate_len); // end range - delete start
							if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
								to_delete_zero_ranges_from = i;
							}

							// the only case we change range.len -= and continue, which effects agg len.
							aggerate_len += (end_range - aggerate_len) - (op_ins - aggerate_len);

							// if range.len == 0 { todo!() } // Remove range if the whole range is deleted.
							dbg!(op_ins );
							dbg!(op_ins - op_len);
							dbg!(range.clone());

							// as start/end ranges are compared direcly we don't want to base op_ins/len based on agg len.
							op_len = op_ins - op_len - (end_range); // delete end - end range
							op_ins = (end_range); // end range
							op_len = -op_len;
							last_op_to_be_delete = true;

							// delete range doesn't get effect as we are just deleting the insert range
						} else if op_ins != end_range { // delete doesn't extends beyond end of range
							range.len -= op_ins - op_len - aggerate_len - (op_ins - aggerate_len); // delete end - delete start
							if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
								to_delete_zero_ranges_from = i;
							}
							dbg!(range.clone());
							// delete range doesn't get effect as we are just deleting the insert range
							range_insertion_index = usize::MAX;
							break;
						} else if i == ops_len-1 {
							// basically in the case op_ins == end_range we want to continue and check for other ifs.
							// really just checking for the last element.
							if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
								orignal_doc_delete_range.ins = op_ins - range.len - aggerate_len; // delete start
								orignal_doc_delete_range.len = (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end
								range_delete_index = i+1;
							} else {
								orignal_doc_delete_range.len += (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
							}
						}
					}
				// } else if op_ins < start_range { // Split new insertion in the sequential list.
				} else if op_ins < start_range { // Split new insertion in the sequential list.
					
					// if instead of else if because of (op_ins != end_range) from before.

					// This shouldn't be possible within original length deleted elements as we can't be before the start range as we manually iter throught it before.
					// [(5,-2),(6,len),(7,len)] as we check first 0 to 3 then 6s and 7s length.
					if op_len.is_positive() {
						new_range = Op {
							ins: op_ins - aggerate_len,
							len: op_len
						};
						range_insertion_index = i;
						break;
					} else {
						if op_ins - op_len - aggerate_len > (start_range - aggerate_len) && op_ins - op_len - aggerate_len <= (end_range - aggerate_len) { // delete extends beyond start range and below end range.
							if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
								orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
								orignal_doc_delete_range.len = (start_range - aggerate_len) - orignal_doc_delete_range.ins; // start range - delete start
								
								range.len -= (op_ins - op_len - aggerate_len) - (start_range - aggerate_len); // delete end - start range
								if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
									to_delete_zero_ranges_from = i;
								}
								range_delete_index = i; // end of delete.
								dbg!(range.clone());
								break;	
							} else { // If delete_range already exists
								orignal_doc_delete_range.len += (start_range - aggerate_len) - (op_ins - aggerate_len); // start range - delete start
								range.len -= (op_ins - op_len - aggerate_len) - (start_range - aggerate_len); // delete end - start range
								if to_delete_zero_ranges_from == usize::MAX && range.len == 0 {
									to_delete_zero_ranges_from = i;
								}
								dbg!(range.clone());
								break;	
							}
						} else if op_ins - op_len - aggerate_len > end_range - aggerate_len { // delete extends beyond end of range, therefore we continue to find deletes.
							if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
								orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
								orignal_doc_delete_range.len = (start_range - aggerate_len) - orignal_doc_delete_range.ins; // start range - delete start
								range_delete_index = i;
								
								range.len = 0; // Basically we have whole range to delete 
								if to_delete_zero_ranges_from == usize::MAX {
									to_delete_zero_ranges_from = i;
								}
								op_len = op_ins - op_len - (end_range); // delete end - end range
								op_ins = (end_range); // end range
								op_len = -op_len;
								last_op_to_be_delete = true;
								dbg!(op_ins);
								dbg!(op_len);
							} else { // If delete_range already exists
								orignal_doc_delete_range.len += (start_range - aggerate_len) - (op_ins - aggerate_len); // start range - delete start
								
								range.len = 0; // Basically we have whole range to delete 
								if to_delete_zero_ranges_from == usize::MAX {
									to_delete_zero_ranges_from = i;
								}

								op_len = op_ins - op_len - (end_range); // delete end - end range
								op_ins = (end_range); // end range
								op_len = -op_len;
								last_op_to_be_delete = true;
								dbg!(op_ins);
								dbg!(op_len);
								// Wait ins't this incorrect, don't we need to change the len to be negative?
							}
						} else { // delete ends before start of range
							if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
								orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
								orignal_doc_delete_range.len = (op_ins - op_len - aggerate_len) - (op_ins - aggerate_len); // delete end - delete start
								dbg!(orignal_doc_delete_range.clone());
								
								range_delete_index = i; // end of delete.
								break;	
							} else { // If delete_range already exists
								orignal_doc_delete_range.len += (op_ins - op_len - aggerate_len) - op_ins - aggerate_len; // delete end - delete start
								dbg!(orignal_doc_delete_range.clone());
								
								break;
							}
						}	
					}
				} else if i == ops_len-1  { // if op_ins > start_range && op_ins < end_range i.g. OP is after the range
					if op_len.is_positive() {
						new_range = Op {
							ins: op_ins - aggerate_len - range.len, // basically if we are inserting after the range, we would also want the range len included in aggerate len
							len: op_len
						};
						range_insertion_index = i+1; // Just insert after the last range
					} else {
						if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
							dbg!("hello 1");
							dbg!(start_range);
							dbg!(end_range);
							dbg!(op_ins - range.len - aggerate_len);
							dbg!((op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len));
							dbg!(-op_len);
							orignal_doc_delete_range.ins = op_ins - range.len - aggerate_len; // delete start
							orignal_doc_delete_range.len = (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end
							range_delete_index = i+1;
						} else {
							orignal_doc_delete_range.len += (op_ins - op_len - range.len - aggerate_len) - (op_ins - range.len - aggerate_len); // delete end - delete start
						}
					}
				}

				aggerate_len += range.len; // range_len is basically effected and therefore future elements for the iter.
			} 

			// in the case of a extending delete but no more iter, this only happens for deletes and at the end.
			if last_op_to_be_delete == true {
				if orignal_doc_delete_range.ins == i32::MAX { // If delete_range doesn't already exists
					orignal_doc_delete_range.ins = op_ins - aggerate_len; // delete start
					orignal_doc_delete_range.len = (op_ins - op_len - aggerate_len) - (op_ins - aggerate_len); // delete end
					range_delete_index = (new_oplist.ops.len()-1)+1;
				} else {
					orignal_doc_delete_range.len += (op_ins - op_len  - aggerate_len) - (op_ins  - aggerate_len); // delete end - delete start
				}
			}


			if range_insertion_index != usize::MAX {
				assert!(new_range != Op { ins: i32::MAX, len: i32::MAX }); // this is more or less a placeholder.
				new_oplist.ops.insert(range_insertion_index as usize, new_range)
			}
			if range_delete_index != usize::MAX { // The reason we can't do this is because in some cases we are deleting futher ranges which doesn't explicialy state.
				assert!(orignal_doc_delete_range != Op { ins: i32::MAX, len: i32::MAX }); // this is more or less a placeholder.
				dbg!(last_delete_range_start);
				dbg!(last_delete_range_end);
				dbg!(previous_delete_range_start);
				dbg!(previous_delete_range_end);
				dbg!(range_delete_index);
				if last_delete_range_end == orignal_doc_delete_range.ins {
					// if delete is extending a previous delete
					new_oplist.ops[last_delete_range_index].len -= orignal_doc_delete_range.len;


					if range_delete_index < new_oplist.ops.len() {
						if (new_oplist.ops[range_delete_index].len.is_negative()) && (new_oplist.ops[range_delete_index].ins == orignal_doc_delete_range.ins + orignal_doc_delete_range.len) {
							new_oplist.ops[last_delete_range_index].len += new_oplist.ops[range_delete_index].len;
							new_oplist.ops.remove(range_delete_index);
						}
					} else if range_delete_index+1 < new_oplist.ops.len() {				
						if (new_oplist.ops[range_delete_index+1].len.is_negative()) && (new_oplist.ops[range_delete_index+1].ins == orignal_doc_delete_range.ins + orignal_doc_delete_range.len) {
							new_oplist.ops[last_delete_range_index].len += new_oplist.ops[range_delete_index+1].len;
							new_oplist.ops.remove(range_delete_index+1);
						}
					}
				}
				// if delete is extending towards the right.
				else if range_delete_index < new_oplist.ops.len() {
					if (new_oplist.ops[range_delete_index].len.is_negative()) && (new_oplist.ops[range_delete_index].ins == orignal_doc_delete_range.ins + orignal_doc_delete_range.len) {
						new_oplist.ops[range_delete_index].ins -= orignal_doc_delete_range.len;
						new_oplist.ops[range_delete_index].len -= orignal_doc_delete_range.len;
					} else if range_delete_index+1 < new_oplist.ops.len() {
						// in the case of 7,1;7,-1 then do this.
						
						if (new_oplist.ops[range_delete_index+1].len.is_negative()) && (new_oplist.ops[range_delete_index+1].ins == orignal_doc_delete_range.ins + orignal_doc_delete_range.len) {
							
							new_oplist.ops[range_delete_index+1].ins -= orignal_doc_delete_range.len;
							new_oplist.ops[range_delete_index+1].len -= orignal_doc_delete_range.len;
						} else {
							// lastly in case we need to split the list and insert the element into place.
		
							orignal_doc_delete_range.len *= -1;
							new_oplist.ops.insert(range_delete_index, orignal_doc_delete_range);
						}
					} else {
						// lastly in case we need to split the list and insert the element into place.
	
						orignal_doc_delete_range.len *= -1;
						new_oplist.ops.insert(range_delete_index, orignal_doc_delete_range);
					} 
				} else {
					// lastly in case we need to split the list and insert the element into place.

					orignal_doc_delete_range.len *= -1;
					new_oplist.ops.insert(range_delete_index, orignal_doc_delete_range);
				}
				dbg!(orignal_doc_delete_range);
				// new_oplist.ops.insert(range_insertion_index as usize, new_range)
			}

			// remove zero length ranges, this has to be done at the end, and removes always takes place afterwords.
			if to_delete_zero_ranges_from != usize::MAX {
				let mut i = to_delete_zero_ranges_from;
				while i < new_oplist.ops.len() {
					if new_oplist.ops[i].len == 0 {
						new_oplist.ops.remove(i);
					} else {
						i+=1;
					}
				}
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
    Continue,
}

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

        // Pre existing = 123457890...
        // After new applied  = 1234578-90...
		let test_vec: OpList = getOpListforTesting([(5,-1)], [(7,1)]);
		let expected_result = getOpList([(5, -1), (8, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 12345-67890...
        // After new applied (=) = 12345-6=7890...
		let test_vec: OpList = getOpListforTesting([(5,1)], [(7,1)]);
		let expected_result = getOpList([(5, 1), (6, 1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test cases for: 1234567 -> 123456-7= -> 12345-=

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345+-=890...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(5,1)]);
		let expected_result = getOpList([(5,1),(5,-2),(6,1),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-+=890...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(6,1)]);
		let expected_result = getOpList([(5,-2),(6,2),(7,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-=8+90...
		let test_vec: OpList = getOpListforTesting([(5,-2),(6,1),(7,1)], [(8,1)]);
		let expected_result = getOpList([(5,-2),(6,1),(7,1),(8,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
		
		// Test cases for 1-2-35-

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-+35-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(4,1)]);
		let expected_result = getOpList([(1,1),(2,2),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-3+5-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(5,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,1),(3,-1),(5,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35+-67890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(6,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,2)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35-6+7890...
		let test_vec: OpList = getOpListforTesting([(1,1),(2,1),(3,-1),(5,1)], [(8,1)]);
		let expected_result = getOpList([(1,1),(2,1),(3,-1),(5,1),(6,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

		// Test for 4,-4 and stuff to check for first elements. 


		// Test for delete RLE

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 12345790...
        // After new applied = 1234590...
		let test_vec: OpList = getOpListforTesting([(5,-1),(7,-1)], [(6,-1)]);
		let expected_result = getOpList([(5,-3)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-=~90...
		let mut test_vec: OpList = getOpListforTesting([(3,-3),(4,1),(5,1),(6,1),(7,-1)], [(7,-1)]);
		let expected_result = getOpList([(3,-5),(4,1),(5,1),(6,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-90...
		let mut test_vec: OpList = getOpListforTesting([(3,-3),(4,1),(5,1),(6,1),(7,-1)], [(7,-3)]);
		let expected_result = getOpList([(3,-5),(4,1)]);
		assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base (if no pre existing) = 1234567890...
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