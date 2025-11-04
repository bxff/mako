"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Tests = exports.TestDataConfig = exports.OpListClass = void 0;
exports.getOpList = getOpList;
exports.getOpListbyVec = getOpListbyVec;
exports.getOpListforTesting = getOpListforTesting;
exports.generate_random_test_data = generate_random_test_data;
exports.generate_random_test_data_with_config = generate_random_test_data_with_config;
exports.generate_only_positive_random_test_data = generate_only_positive_random_test_data;
// Configuration for random test data generation
class TestDataConfig {
    constructor() {
        this.numTests = 10;
        this.minInsertPos = 1;
        this.maxInsertPos = 10;
        this.minLength = -5;
        this.maxLength = 5;
    }
}
exports.TestDataConfig = TestDataConfig;
/// Creates an OpList from a fixed-size array of operations
function getOpList(list) {
    return {
        ops: list.map(([ins, len]) => ({ ins, len })),
        testOp: null
    };
}
/// Creates an OpList from a Vec of operations
function getOpListbyVec(list) {
    return {
        ops: list.map(([ins, len]) => ({ ins, len })),
        testOp: null
    };
}
/// Creates an OpList for testing with pre-existing range list
function getOpListforTesting(pre_existing_range_list, oplist) {
    return {
        ops: oplist.map(([ins, len]) => ({ ins, len })),
        testOp: pre_existing_range_list.map(([ins, len]) => [ins, len])
    };
}
class OpListClass {
    constructor(opList) {
        this.ops = opList.ops;
        this.testOp = opList.testOp;
    }
    /// Helper function to initialize the new OpList with the first operation
    initialize_new_oplist() {
        // Create empty OpList to be populated
        let new_oplist = new OpListClass({ ops: [], testOp: null });
        // Determine starting index based on three conditions
        let start_index;
        if (this.testOp !== null) {
            // Condition 1: Use test data if available
            new_oplist = new OpListClass(getOpListbyVec(this.testOp));
            start_index = 0; // Start from beginning since we're using all test data
        }
        else if (this.ops.length === 0) {
            // Condition 2: No operations to process
            start_index = 0; // Start from beginning with empty list
        }
        else {
            // Condition 3: Process first operation normally
            const first_op = this.ops[0];
            const initial_op = first_op.len < 0 ?
                // For delete operations: normalize position by adding length
                // Example: ins=5, len=-2 becomes ins=3, len=-2
                {
                    ins: first_op.ins + first_op.len,
                    len: first_op.len
                } :
                // For insert operations: keep original position
                {
                    ins: first_op.ins,
                    len: first_op.len
                };
            new_oplist.ops.push(initial_op);
            start_index = 1; // Start from second operation since first is processed
        }
        return [new_oplist, start_index];
    }
    /// Helper function to normalize delete operation position
    static normalize_delete_op(op) {
        if (op.len < 0) {
            return {
                ins: op.ins + op.len,
                len: op.len
            };
        }
        else {
            return { ...op };
        }
    }
    /// Helper function to check if an operation is within a range
    static is_op_in_range(op_ins, start_range, end_range) {
        return op_ins >= start_range && op_ins <= end_range;
    }
    /// Helper function to remove zero-length ranges
    /// More efficient than removing one by one as it uses retain
    static remove_zero_length_ranges(ops, start_index) {
        if (start_index >= ops.length) {
            return;
        }
        // Split the vector and only process the part from start_index
        const temp_ops = ops.splice(start_index);
        const filtered = temp_ops.filter(op => op.len !== 0);
        ops.push(...filtered);
    }
    /// Helper function to handle the last element logic that was repeated in multiple places
    /// This reduces code duplication for the `if i == new_oplist_len - 1` blocks
    static handle_last_element_logic(op_ins, op_len, range, aggregate_len, original_doc_delete_range, range_delete_index, new_range, range_insertion_index, i) {
        if (op_len > 0) {
            new_range.ins = op_ins - aggregate_len - range.len;
            new_range.len = op_len;
            range_insertion_index.value = i + 1;
        }
        else {
            if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                original_doc_delete_range.ins = op_ins - range.len - aggregate_len;
                original_doc_delete_range.len = (op_ins - op_len - range.len - aggregate_len) - (op_ins - range.len - aggregate_len);
                range_delete_index.value = i + 1;
            }
            else {
                original_doc_delete_range.len += (op_ins - op_len - range.len - aggregate_len) - (op_ins - range.len - aggregate_len);
            }
        }
    }
    /// Returns a new OpList with discontinuous ranges. This transforms operations
    /// from the current document state to be expressed in base document coordinates.
    ///
    /// This method takes operations that are relative to the current document state
    /// and converts them to be relative to the original base document (position 0).
    ///
    /// Example transformation:
    /// Input: [(5,1), (7,1)] -> Output: [(5,1), (6,1)]
    ///
    /// The algorithm processes each operation by finding where it fits within existing
    /// ranges and adjusting positions accordingly. It handles both insert operations
    /// (positive length) and delete operations (negative length) with complex merging
    /// logic for adjacent operations.
    from_oplist_to_sequential_list() {
        // Initialize the new OpList with existing state
        const [new_oplist, start_from_for_ops] = this.initialize_new_oplist();
        // Process each operation from the starting point
        for (const op of this.ops.slice(start_from_for_ops)) {
            // Normalize delete operation position for base coordinates
            const normalized_op = OpListClass.normalize_delete_op(op);
            let op_ins = normalized_op.ins;
            let op_len = normalized_op.len;
            // === STATE TRACKING VARIABLES ===
            // These variables track the processing state for each operation
            let aggregate_len = 0; // Cumulative length adjustment from processed ranges
            const range_insertion_index = { value: Number.MAX_SAFE_INTEGER }; // Where to insert new insert operations
            const range_delete_index = { value: Number.MAX_SAFE_INTEGER }; // Where to insert new delete operations
            let start_range; // Start boundary of current range in base coords
            let end_range = Number.MAX_SAFE_INTEGER; // End boundary of current range in base coords
            // === OPERATION TRACKING ===
            const new_range = { ins: Number.MAX_SAFE_INTEGER, len: Number.MAX_SAFE_INTEGER }; // New operation to insert
            const original_doc_delete_range = { ins: Number.MAX_SAFE_INTEGER, len: Number.MAX_SAFE_INTEGER }; // Delete operation in base coords
            let last_op_to_be_delete = false; // Flag for incomplete delete processing
            let to_delete_zero_ranges_from = Number.MAX_SAFE_INTEGER; // Starting index for zero-length range cleanup
            // === DELETE RANGE TRACKING ===
            // Track previous delete ranges for complex overlap scenarios
            let previous_delete_range_start = Number.MAX_SAFE_INTEGER;
            let previous_delete_range_end = Number.MAX_SAFE_INTEGER;
            let last_delete_range_start = Number.MAX_SAFE_INTEGER;
            let last_delete_range_end = Number.MAX_SAFE_INTEGER;
            let last_delete_range_index = Number.MAX_SAFE_INTEGER;
            const new_oplist_len = new_oplist.ops.length;
            // === MAIN RANGE PROCESSING LOOP ===
            // Iterate through each existing range to determine where the new operation fits
            for (let i = 0; i < new_oplist.ops.length; i++) {
                const range = new_oplist.ops[i];
                last_op_to_be_delete = false;
                // === HANDLE DELETE RANGES (negative length) ===
                if (range.len < 0) {
                    // Track current delete range boundaries
                    previous_delete_range_start = range.ins;
                    previous_delete_range_end = previous_delete_range_start + (-range.len);
                    // Calculate range boundaries in base coordinates
                    start_range = end_range !== Number.MAX_SAFE_INTEGER ?
                        end_range : // Use previous end as current start
                        0; // Start from beginning for first range
                    // Validate delete range position
                    if ((range.ins + aggregate_len) > 0) {
                        end_range = range.ins + aggregate_len;
                    }
                    else {
                        throw new Error("Deletes should delete into the negative range, e.g. (4,-5) shouldn't exist.");
                    }
                    // Check if current operation falls within this delete range
                    if (OpListClass.is_op_in_range(op_ins, start_range, end_range)) {
                        if (op_len > 0) {
                            // === INSERT OPERATION WITHIN DELETE RANGE ===
                            new_range.ins = op_ins - aggregate_len; // Convert to base coordinates
                            new_range.len = op_len;
                            range_insertion_index.value = i;
                            break; // Operation placed, stop processing
                        }
                        else {
                            // === DELETE OPERATION WITHIN DELETE RANGE ===
                            // Complex logic for delete-over-delete scenarios
                            if (op_ins - op_len > end_range && (op_ins !== end_range)) {
                                // Delete extends beyond current range end
                                if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                                    original_doc_delete_range.ins = op_ins - aggregate_len;
                                    original_doc_delete_range.len = end_range - aggregate_len - original_doc_delete_range.ins;
                                    range_delete_index.value = i;
                                    original_doc_delete_range.len -= range.len;
                                    aggregate_len += range.len;
                                    range.len = 0; // Mark range for removal
                                    if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER && range.len === 0) {
                                        to_delete_zero_ranges_from = i;
                                    }
                                    // Adjust remaining operation
                                    op_len = op_ins - op_len - end_range;
                                    op_ins = end_range;
                                    op_len = -op_len;
                                    last_op_to_be_delete = true;
                                }
                                else {
                                    original_doc_delete_range.len += (end_range - aggregate_len) - (op_ins - aggregate_len);
                                    op_len = op_ins - op_len - end_range;
                                    op_ins = end_range;
                                    op_len = -op_len;
                                    last_op_to_be_delete = true;
                                }
                            }
                            else if (op_ins !== end_range) {
                                // Delete doesn't extend beyond range end
                                if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                                    original_doc_delete_range.ins = op_ins - aggregate_len;
                                    original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                                    range_delete_index.value = i;
                                    break;
                                }
                                else {
                                    original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                                    break;
                                }
                            }
                            else if (i === new_oplist_len - 1) {
                                // Last element special case
                                OpListClass.handle_last_element_logic(op_ins, op_len, range, aggregate_len, original_doc_delete_range, range_delete_index, new_range, range_insertion_index, i);
                            }
                        }
                    }
                    else if (i === new_oplist_len - 1) {
                        // Operation is after this range (last range case)
                        OpListClass.handle_last_element_logic(op_ins, op_len, range, aggregate_len, original_doc_delete_range, range_delete_index, new_range, range_insertion_index, i);
                    }
                    // Update delete range tracking for next iteration
                    last_delete_range_start = range.ins;
                    last_delete_range_end = last_delete_range_start + (-range.len);
                    last_delete_range_index = i;
                    aggregate_len += range.len;
                    continue; // Move to next range
                }
                // === HANDLE INSERT RANGES (positive length) ===
                // Adjust start position for ranges affected by previous deletes
                if (range.ins > previous_delete_range_start && range.ins <= previous_delete_range_end) {
                    start_range = previous_delete_range_start +
                        (previous_delete_range_end - previous_delete_range_start) + aggregate_len;
                }
                else {
                    start_range = range.ins + aggregate_len;
                }
                end_range = start_range + range.len;
                // Check if current operation falls within this insert range
                if (OpListClass.is_op_in_range(op_ins, start_range, end_range)) {
                    if (op_len > 0) {
                        // === INSERT OPERATION WITHIN INSERT RANGE ===
                        // Extend existing range (run-length encoding)
                        range.len += op_len;
                        range_insertion_index.value = Number.MAX_SAFE_INTEGER; // Mark as merged
                        break;
                    }
                    else {
                        // === DELETE OPERATION WITHIN INSERT RANGE ===
                        if (op_ins - op_len - aggregate_len > (end_range - aggregate_len) && (op_ins !== end_range)) {
                            // Delete extends beyond range end
                            range.len -= (end_range - aggregate_len) - (op_ins - aggregate_len);
                            if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER && range.len === 0) {
                                to_delete_zero_ranges_from = i;
                            }
                            aggregate_len += (end_range - aggregate_len) - (op_ins - aggregate_len);
                            op_len = op_ins - op_len - end_range;
                            op_ins = end_range;
                            op_len = -op_len;
                            last_op_to_be_delete = true;
                        }
                        else if (op_ins !== end_range) {
                            // Delete doesn't extend beyond range end
                            range.len -= op_ins - op_len - aggregate_len - (op_ins - aggregate_len);
                            if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER && range.len === 0) {
                                to_delete_zero_ranges_from = i;
                            }
                            range_insertion_index.value = Number.MAX_SAFE_INTEGER;
                            break;
                        }
                        else if (i === new_oplist_len - 1) {
                            OpListClass.handle_last_element_logic(op_ins, op_len, range, aggregate_len, original_doc_delete_range, range_delete_index, new_range, range_insertion_index, i);
                        }
                    }
                }
                else if (op_ins < start_range) {
                    // === OPERATION BEFORE CURRENT RANGE ===
                    if (op_len > 0) {
                        // Insert before this range
                        new_range.ins = op_ins - aggregate_len;
                        new_range.len = op_len;
                        range_insertion_index.value = i;
                        break;
                    }
                    else {
                        // === DELETE BEFORE CURRENT RANGE ===
                        // Complex overlap logic for delete spanning multiple ranges
                        if (op_ins - op_len - aggregate_len > (start_range - aggregate_len) &&
                            op_ins - op_len - aggregate_len <= (end_range - aggregate_len)) {
                            // Delete overlaps with range start
                            if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                                original_doc_delete_range.ins = op_ins - aggregate_len;
                                original_doc_delete_range.len = (start_range - aggregate_len) - original_doc_delete_range.ins;
                                range.len -= (op_ins - op_len - aggregate_len) - (start_range - aggregate_len);
                                if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER && range.len === 0) {
                                    to_delete_zero_ranges_from = i;
                                }
                                range_delete_index.value = i;
                                break;
                            }
                            else {
                                original_doc_delete_range.len += (start_range - aggregate_len) - (op_ins - aggregate_len);
                                range.len -= (op_ins - op_len - aggregate_len) - (start_range - aggregate_len);
                                if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER && range.len === 0) {
                                    to_delete_zero_ranges_from = i;
                                }
                                break;
                            }
                        }
                        else if (op_ins - op_len - aggregate_len > end_range - aggregate_len) {
                            // Delete extends beyond range entirely
                            if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                                original_doc_delete_range.ins = op_ins - aggregate_len;
                                original_doc_delete_range.len = (start_range - aggregate_len) - original_doc_delete_range.ins;
                                range_delete_index.value = i;
                                range.len = 0; // Mark entire range for deletion
                                if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER) {
                                    to_delete_zero_ranges_from = i;
                                }
                                op_len = op_ins - op_len - end_range;
                                op_ins = end_range;
                                op_len = -op_len;
                                last_op_to_be_delete = true;
                            }
                            else {
                                original_doc_delete_range.len += (start_range - aggregate_len) - (op_ins - aggregate_len);
                                range.len = 0;
                                if (to_delete_zero_ranges_from === Number.MAX_SAFE_INTEGER) {
                                    to_delete_zero_ranges_from = i;
                                }
                                op_len = op_ins - op_len - end_range;
                                op_ins = end_range;
                                op_len = -op_len;
                                last_op_to_be_delete = true;
                            }
                        }
                        else {
                            // Delete ends before range start
                            if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                                original_doc_delete_range.ins = op_ins - aggregate_len;
                                original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                                range_delete_index.value = i;
                                break;
                            }
                            else {
                                original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                                break;
                            }
                        }
                    }
                }
                else if (i === new_oplist_len - 1) {
                    // === OPERATION AFTER ALL RANGES ===
                    OpListClass.handle_last_element_logic(op_ins, op_len, range, aggregate_len, original_doc_delete_range, range_delete_index, new_range, range_insertion_index, i);
                }
                aggregate_len += range.len;
            }
            // === POST-PROCESSING ===
            // Handle any remaining delete operation that couldn't be processed
            if (last_op_to_be_delete) {
                if (original_doc_delete_range.ins === Number.MAX_SAFE_INTEGER) {
                    original_doc_delete_range.ins = op_ins - aggregate_len;
                    original_doc_delete_range.len = (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                    range_delete_index.value = new_oplist.ops.length;
                }
                else {
                    original_doc_delete_range.len += (op_ins - op_len - aggregate_len) - (op_ins - aggregate_len);
                }
            }
            // Insert new range if needed
            if (range_insertion_index.value !== Number.MAX_SAFE_INTEGER) {
                console.assert(new_range.ins !== Number.MAX_SAFE_INTEGER);
                new_oplist.ops.splice(range_insertion_index.value, 0, new_range);
            }
            // Handle delete range insertion with complex merging logic
            if (range_delete_index.value !== Number.MAX_SAFE_INTEGER) {
                this.insert_delete_range_with_merging(new_oplist, original_doc_delete_range, range_delete_index.value, last_delete_range_end, last_delete_range_index);
            }
            // Clean up zero-length ranges efficiently
            if (to_delete_zero_ranges_from !== Number.MAX_SAFE_INTEGER) {
                OpListClass.remove_zero_length_ranges(new_oplist.ops, to_delete_zero_ranges_from);
            }
        }
        return new_oplist;
    }
    /// Inserts a delete range with complex merging logic for adjacent deletes
    insert_delete_range_with_merging(new_oplist, original_doc_delete_range, range_delete_index, last_delete_range_end, last_delete_range_index) {
        console.assert(original_doc_delete_range.ins !== Number.MAX_SAFE_INTEGER);
        // Check if delete extends previous delete (merge adjacent deletes)
        if (last_delete_range_end === original_doc_delete_range.ins) {
            new_oplist.ops[last_delete_range_index].len -= original_doc_delete_range.len;
            this.merge_adjacent_delete_ranges(new_oplist, original_doc_delete_range, range_delete_index, last_delete_range_index);
        }
        else {
            this.insert_delete_range_at_position(new_oplist, original_doc_delete_range, range_delete_index);
        }
    }
    /// Merges adjacent delete ranges to optimize the operation list
    merge_adjacent_delete_ranges(new_oplist, original_doc_delete_range, range_delete_index, last_delete_range_index) {
        // Check for adjacent ranges that can be merged
        if (range_delete_index < new_oplist.ops.length) {
            const range_at_index = new_oplist.ops[range_delete_index];
            if (range_at_index.len < 0 &&
                range_at_index.ins === original_doc_delete_range.ins + original_doc_delete_range.len) {
                new_oplist.ops[last_delete_range_index].len += range_at_index.len;
                new_oplist.ops.splice(range_delete_index, 1);
            }
        }
        else if (range_delete_index + 1 < new_oplist.ops.length) {
            const next_range = new_oplist.ops[range_delete_index + 1];
            if (next_range.len < 0 &&
                next_range.ins === original_doc_delete_range.ins + original_doc_delete_range.len) {
                new_oplist.ops[last_delete_range_index].len += next_range.len;
                new_oplist.ops.splice(range_delete_index + 1, 1);
            }
        }
    }
    /// Inserts delete range at the specified position with appropriate merging
    insert_delete_range_at_position(new_oplist, original_doc_delete_range, range_delete_index) {
        if (range_delete_index < new_oplist.ops.length) {
            // Handle insertion at various positions with merge logic
            const range_at_index = new_oplist.ops[range_delete_index];
            if (range_at_index.len < 0 &&
                range_at_index.ins === original_doc_delete_range.ins + original_doc_delete_range.len) {
                range_at_index.ins -= original_doc_delete_range.len;
                range_at_index.len -= original_doc_delete_range.len;
            }
            else if (range_delete_index + 1 < new_oplist.ops.length) {
                const next_range = new_oplist.ops[range_delete_index + 1];
                if (next_range.len < 0 &&
                    next_range.ins === original_doc_delete_range.ins + original_doc_delete_range.len) {
                    next_range.ins -= original_doc_delete_range.len;
                    next_range.len -= original_doc_delete_range.len;
                }
                else {
                    original_doc_delete_range.len *= -1;
                    new_oplist.ops.splice(range_delete_index, 0, original_doc_delete_range);
                }
            }
            else {
                original_doc_delete_range.len *= -1;
                new_oplist.ops.splice(range_delete_index, 0, original_doc_delete_range);
            }
        }
        else {
            original_doc_delete_range.len *= -1;
            new_oplist.ops.splice(range_delete_index, 0, original_doc_delete_range);
        }
    }
    /// Convert sequential_list into a oplist
    from_sequential_list_to_oplist() {
        // incorrect
        for (let i = 1; i < this.ops.length; i++) {
            this.ops[i].ins += this.ops[i - 1].len;
        }
    }
    /// Changes delete ranges such as 2,-1 to 1,2 to 1,-2.
    /// Should only be used for reading output for testing.
    clean_delete() {
        for (const op of this.ops) {
            if (op.len < 0) {
                const ins = op.ins;
                op.ins = op.ins + op.len;
                op.len = -ins;
            }
        }
    }
    /// Human readable output for testing.
    clean_output() {
        throw new Error("Not implemented"); // maybe not worthwhile
    }
    clone() {
        return new OpListClass({
            ops: [...this.ops.map(op => ({ ...op }))],
            testOp: this.testOp ? [...this.testOp] : null
        });
    }
}
exports.OpListClass = OpListClass;
/// Generates random test data for testing operations
/// Note: This is not implemented correctly yet, specifically inclusive range is random.
/// This is very poorly optimized but shouldn't matter for testing.
function generate_random_test_data(num_tests) {
    const config = new TestDataConfig();
    config.numTests = num_tests;
    return generate_random_test_data_with_config(config);
}
/// Generates random test data with custom configuration
function generate_random_test_data_with_config(config) {
    const test_data = [];
    for (let i = 0; i < config.numTests; i++) {
        const ins = Math.floor(Math.random() * (config.maxInsertPos - config.minInsertPos + 1)) + config.minInsertPos;
        const len = Math.floor(Math.random() * (config.maxLength - config.minLength + 1)) + config.minLength;
        // Skip invalid operations
        if (len === 0 || (len + ins) < 0) {
            continue;
        }
        test_data.push({ ins, len });
    }
    return new OpListClass({ ops: test_data, testOp: null });
}
/// Old code: generating only positive random test data for testing.
function generate_only_positive_random_test_data(num_tests) {
    const test_data = [];
    for (let i = 0; i < num_tests; i++) {
        const ins = Math.floor(Math.random() * 10) + 1;
        const len = Math.floor(Math.random() * 10) + 1;
        test_data.push({ ins, len });
    }
    return new OpListClass({ ops: test_data, testOp: null });
}
// Test cases
class Tests {
    static test_whats_already_implemented() {
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
        // Helper function to create tuples for test data
        const t = (a, b) => [a, b];
        // Base = 1234567890...
        // Pre existing = 123457890...
        // After new applied (+) = 1234578+90...
        let test_vec = new OpListClass(getOpListforTesting([t(5, -1)], [t(7, 1)]));
        let expected_result = new OpListClass(getOpList([t(5, -1), t(8, 1)]));
        const result1 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result1, expected_result)) {
            console.error("Test 1 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result1.ops);
            throw new Error("Test 1 failed");
        }
        // Base = 1234567890...
        // Pre existing = 12345-67890...
        // After new applied (+) = 12345-6+7890...
        test_vec = new OpListClass(getOpListforTesting([t(5, 1)], [t(7, 1)]));
        expected_result = new OpListClass(getOpList([t(5, 1), t(6, 1)]));
        const result2 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result2, expected_result)) {
            console.error("Test 2 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result2.ops);
            throw new Error("Test 2 failed");
        }
        // Test cases for: 1234567 -> 123456-7= -> 12345-=
        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345+-=890...
        test_vec = new OpListClass(getOpListforTesting([t(5, -2), t(6, 1), t(7, 1)], [t(5, 1)]));
        expected_result = new OpListClass(getOpList([t(5, 1), t(5, -2), t(6, 1), t(7, 1)]));
        const result3 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result3, expected_result)) {
            console.error("Test 3 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result3.ops);
            throw new Error("Test 3 failed");
        }
        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-+=890...
        test_vec = new OpListClass(getOpListforTesting([t(5, -2), t(6, 1), t(7, 1)], [t(6, 1)]));
        expected_result = new OpListClass(getOpList([t(5, -2), t(6, 2), t(7, 1)]));
        const result4 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result4, expected_result)) {
            console.error("Test 4 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result4.ops);
            throw new Error("Test 4 failed");
        }
        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-=8+90...
        test_vec = new OpListClass(getOpListforTesting([t(5, -2), t(6, 1), t(7, 1)], [t(8, 1)]));
        expected_result = new OpListClass(getOpList([t(5, -2), t(6, 1), t(7, 1), t(8, 1)]));
        const result5 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result5, expected_result)) {
            console.error("Test 5 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result5.ops);
            throw new Error("Test 5 failed");
        }
        // Test cases for 1-2-35-
        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-+35-67890...
        test_vec = new OpListClass(getOpListforTesting([t(1, 1), t(2, 1), t(3, -1), t(5, 1)], [t(4, 1)]));
        expected_result = new OpListClass(getOpList([t(1, 1), t(2, 2), t(3, -1), t(5, 1)]));
        const result6 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result6, expected_result)) {
            console.error("Test 6 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result6.ops);
            throw new Error("Test 6 failed");
        }
        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-3+5-67890...
        test_vec = new OpListClass(getOpListforTesting([t(1, 1), t(2, 1), t(3, -1), t(5, 1)], [t(5, 1)]));
        expected_result = new OpListClass(getOpList([t(1, 1), t(2, 1), t(3, 1), t(3, -1), t(5, 1)]));
        const result7 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result7, expected_result)) {
            console.error("Test 7 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result7.ops);
            throw new Error("Test 7 failed");
        }
        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35+-67890...
        test_vec = new OpListClass(getOpListforTesting([t(1, 1), t(2, 1), t(3, -1), t(5, 1)], [t(6, 1)]));
        expected_result = new OpListClass(getOpList([t(1, 1), t(2, 1), t(3, -1), t(5, 2)]));
        const result8 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result8, expected_result)) {
            console.error("Test 8 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result8.ops);
            throw new Error("Test 8 failed");
        }
        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35-6+7890...
        test_vec = new OpListClass(getOpListforTesting([t(1, 1), t(2, 1), t(3, -1), t(5, 1)], [t(8, 1)]));
        expected_result = new OpListClass(getOpList([t(1, 1), t(2, 1), t(3, -1), t(5, 1), t(6, 1)]));
        const result9 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result9, expected_result)) {
            console.error("Test 9 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result9.ops);
            throw new Error("Test 9 failed");
        }
        // Test for delete RLE
        // Base = 1234567890...
        // Pre existing = 12345790...
        // After new applied = 1234590...
        test_vec = new OpListClass(getOpListforTesting([t(5, -1), t(7, -1)], [t(6, -1)]));
        expected_result = new OpListClass(getOpList([t(5, -3)]));
        const result10 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result10, expected_result)) {
            console.error("Test 10 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result10.ops);
            throw new Error("Test 10 failed");
        }
        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-=~90...
        test_vec = new OpListClass(getOpListforTesting([t(3, -3), t(4, 1), t(5, 1), t(6, 1), t(7, -1)], [t(7, -1)]));
        expected_result = new OpListClass(getOpList([t(3, -5), t(4, 1), t(5, 1), t(6, 1)]));
        const result11 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result11, expected_result)) {
            console.error("Test 11 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result11.ops);
            throw new Error("Test 11 failed");
        }
        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-90...
        test_vec = new OpListClass(getOpListforTesting([t(3, -3), t(4, 1), t(5, 1), t(6, 1), t(7, -1)], [t(7, -3)]));
        expected_result = new OpListClass(getOpList([t(3, -5), t(4, 1)]));
        const result12 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result12, expected_result)) {
            console.error("Test 12 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result12.ops);
            throw new Error("Test 12 failed");
        }
        // Base = 1234567890...
        // Pre existing = 1234567-90...
        // After new applied = 12345690...
        test_vec = new OpListClass(getOpListforTesting([t(7, 1), t(7, -1)], [t(8, -2)]));
        expected_result = new OpListClass(getOpList([t(6, -2)]));
        const result13 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result13, expected_result)) {
            console.error("Test 13 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result13.ops);
            throw new Error("Test 13 failed");
        }
        // Base = 1234567890...
        // Test = 14678-=~90...
        // Expected = 14678-=~90...
        test_vec = new OpListClass(getOpList([t(5, -1), t(3, -1), t(6, 3), t(2, -1)])); // hard to understand
        expected_result = new OpListClass(getOpList([t(1, -2), t(4, -1), t(8, 3)]));
        const result14 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result14, expected_result)) {
            console.error("Test 14 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result14.ops);
            throw new Error("Test 14 failed");
        }
        // Base = 1234567890...
        // Test = 127890...
        // Expected = 127890...
        test_vec = new OpListClass(getOpList([t(5, -2), t(4, -2)])); // 1234567 -> 12367 -> 127; Testing for delete RLE within delete RLE
        expected_result = new OpListClass(getOpList([t(2, -4)]));
        const result15 = test_vec.from_oplist_to_sequential_list();
        if (!this.opListEquals(result15, expected_result)) {
            console.error("Test 15 failed:");
            console.error("Expected:", expected_result.ops);
            console.error("Got:", result15.ops);
            throw new Error("Test 15 failed");
        }
        console.log("✅ All tests passed!");
    }
    static opListEquals(a, b) {
        if (a.ops.length !== b.ops.length) {
            return false;
        }
        for (let i = 0; i < a.ops.length; i++) {
            if (a.ops[i].ins !== b.ops[i].ins || a.ops[i].len !== b.ops[i].len) {
                return false;
            }
        }
        return true;
    }
}
exports.Tests = Tests;
// Main function equivalent
function main() {
    // let mut test_vec: OpList = getOpList([(5,-2),(4,-2)]); // 1234567 -> 12367 -> 127
    // RangeHashMap::todo();
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
    console.log("Running TypeScript implementation of mako...");
}
// Run the tests
if (require.main === module) {
    main();
    Tests.test_whats_already_implemented();
}
