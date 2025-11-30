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
        ops: list.into_iter().map(|(ins, len)| Op { ins, len }).collect(),
        test_op: None,
    }
}

/// Builds an `OpList` from a runtime `Vec` of (position, length) tuples while clearing any testing state.
fn getOpListbyVec(list: Vec<(InsertPos, Length)>) -> OpList {
    OpList {
        ops: list.into_iter().map(|(ins, len)| Op { ins, len }).collect(),
        test_op: None,
    }
}

/// Builds an `OpList` and seeds it with a pre-existing sequential list for testing.
fn getOpListforTesting<const N: usize, const M: usize>(
    pre_existing_range_list: [(InsertPos, Length); N],
    oplist: [(InsertPos, Length); M],
) -> OpList {
    OpList {
        ops: oplist
            .into_iter()
            .map(|(ins, len)| Op { ins, len })
            .collect(),
        test_op: Some(
            pre_existing_range_list
                .into_iter()
                .map(|(ins, len)| (ins, len))
                .collect(),
        ),
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
            .map(|ops| {
                ops.iter()
                    .map(|(ins, len)| Op {
                        ins: *ins,
                        len: *len,
                    })
                    .collect()
            })
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

        OpList {
            ops: ranges,
            test_op: None,
        }
    }

    /// Applies `self` on top of a prior `OpList`, adjusting for all offsets so the result mirrors baseline order.
    fn backwards_apply(&self, prior: &OpList) -> OpList {
        let mut merged = prior.clone();
        let ranges = &mut merged.ops;
        let mut base_cursor: i64 = 0;
        let mut doc_cursor: i64 = 0;
        let mut cumulative_shift_all: i64 = 0;
        let mut cumulative_shift_deletes: i64 = 0;
        let mut prior_ops_iter = prior.ops.iter().peekable();

        for range in &self.ops {
            if range.len == 0 {
                continue;
            }

            let range_base = i64::from(range.ins);
            if range_base > base_cursor {
                let advance = range_base - base_cursor;
                doc_cursor += advance;
                base_cursor = range_base;
            }

            // Process prior operations that affect this range's base position
            while let Some(prior_op) = prior_ops_iter.peek() {
                let prior_base = i64::from(prior_op.ins);
                let effective_base = if prior_op.len > 0 {
                    prior_base
                } else {
                    // For deletes, the effective base is at the end of the deleted range
                    prior_base + i64::from(-prior_op.len)
                };

                if effective_base <= range_base {
                    let prior_op = *prior_ops_iter.next().unwrap();
                    if prior_op.len > 0 {
                        cumulative_shift_all += i64::from(prior_op.len);
                    } else {
                        let delete_len = -i64::from(prior_op.len);
                        cumulative_shift_all += -delete_len;
                        cumulative_shift_deletes += -delete_len;
                    }
                } else {
                    break;
                }
            }

            let adjusted_cursor = if range.len > 0 {
                doc_cursor + cumulative_shift_deletes
            } else {
                doc_cursor + cumulative_shift_all
            };

            if range.len > 0 {
                let ins: InsertPos = adjusted_cursor.try_into().expect("insert cursor overflow");
                Self::apply_insert(ranges, ins, range.len);
                doc_cursor += i64::from(range.len);
            } else {
                let delete_len = -i64::from(range.len);
                let delete_start = adjusted_cursor;
                let start: InsertPos = delete_start.try_into().expect("delete start overflow");
                let len: Length = delete_len.try_into().expect("delete len overflow");
                Self::apply_delete(ranges, start, len);
                base_cursor += delete_len;
            }
        }

        merged.test_op = None;
        merged
    }

    /// Converts a sequential range list back into the user-facing op list, compacting along the way.
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
                Self::write_op(
                    &mut self.ops,
                    write_idx,
                    Op {
                        ins,
                        len: range.len,
                    },
                );
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
                let (overlap_len, overlap_start) =
                    Self::segment_overlap(doc_cursor, gap_len, delete_cursor, delete_end);
                if overlap_len > 0 {
                    let base_offset = overlap_start - doc_cursor;
                    let base_start = base_cursor + base_offset;
                    Self::emit_delete_op(
                        ranges,
                        &mut write_idx,
                        &mut last_delete_idx,
                        DeleteEmit::DocSpan {
                            base_start,
                            len: overlap_len,
                        },
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
                let (overlap_len, _) =
                    Self::segment_overlap(doc_cursor, seg_len, delete_cursor, delete_end);
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
                DeleteEmit::DocSpan {
                    base_start,
                    len: overlap_len,
                },
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
                    return PositionRef::Base {
                        base: base as InsertPos,
                        index,
                    };
                }
                doc_cursor += gap;
                base_cursor = range_base;
            }

            if range.len < 0 {
                if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
                    return PositionRef::Base {
                        base: range.ins,
                        index,
                    };
                }
                base_cursor += i64::from(-range.len);
                continue;
            } else {
                let insert_len = range.len as i64;
                if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
                    return PositionRef::Base {
                        base: range.ins,
                        index,
                    };
                }
                if matches!(bias, LocateBias::PreferOutsideInsert)
                    && target == doc_cursor + insert_len
                {
                    return PositionRef::Insert {
                        index,
                        offset: range.len,
                    };
                }
                if target < doc_cursor + insert_len
                    && (matches!(bias, LocateBias::PreferInsideInsert) || target > doc_cursor)
                {
                    let offset = target - doc_cursor;
                    return PositionRef::Insert {
                        index,
                        offset: offset as Length,
                    };
                }
                doc_cursor += insert_len;
            }
        }

        let base = base_cursor + (target - doc_cursor);
        PositionRef::Base {
            base: base as InsertPos,
            index: ranges.len(),
        }
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
    fn segment_overlap(
        seg_start: i64,
        seg_len: i64,
        delete_cursor: i64,
        delete_end: i64,
    ) -> (i64, i64) {
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

    /// Transforms another sequential list against `self`.
    /// `self` is the base transformation. `other` is the operation to transform.
    /// Returns a new `OpList` representing `other` applied after `self`.
    fn transform(&self, other: &OpList) -> OpList {
        self.transform_impl(other, true)
    }

    /// Applies a transformation on the sequential list.
    /// `transformer` is the operation to apply on `self`.
    fn apply_transformation(&mut self, transformer: &OpList) {
        let new_ops = transformer.transform_impl(self, false);
        self.ops = new_ops.ops;
    }

    fn transform_impl(&self, other: &OpList, shift_on_tie: bool) -> OpList {
        let mut res_ops = Vec::new();
        let s_ops = &self.ops;
        let mut s_i = 0;
        let mut cumulative_shift: i64 = 0;

        for op in &other.ops {
            let target = op.ins as i64;

            // Advance s_i to target
            while s_i < s_ops.len() {
                let sop = s_ops[s_i];
                let sop_ins = sop.ins as i64;
                if sop_ins > target {
                    break;
                }

                if sop.len < 0 {
                    let sop_end = sop_ins - sop.len as i64;
                    if sop_end > target {
                        // Overlaps target. Don't consume.
                        break;
                    }
                    // Fully before target
                    cumulative_shift += sop.len as i64;
                    s_i += 1;
                } else {
                    // Insert
                    if sop_ins == target && !shift_on_tie {
                        break;
                    }
                    cumulative_shift += sop.len as i64;
                    s_i += 1;
                }
            }

            if op.len > 0 {
                let mut mapped_pos = target + cumulative_shift;
                let mut temp_s_i = s_i;

                while temp_s_i < s_ops.len() {
                    let sop = s_ops[temp_s_i];
                    let sop_ins = sop.ins as i64;
                    if sop_ins > target {
                        break;
                    }

                    if sop.len > 0 {
                        if sop_ins == target && !shift_on_tie {
                            // Don't shift for inserts at target if !shift_on_tie
                        } else {
                            mapped_pos += sop.len as i64;
                        }
                    } else {
                        let sop_end = sop_ins - sop.len as i64;
                        if sop_ins <= target && target < sop_end {
                            mapped_pos -= target - sop_ins;
                        }
                    }
                    temp_s_i += 1;
                }

                let ins: InsertPos = mapped_pos.try_into().expect("transform insert overflow");
                Self::push_op(&mut res_ops, Op { ins, len: op.len });
            } else {
                let del_len = -op.len as i64;
                let del_end = target + del_len;
                let mut curr = target;
                let mut temp_s_i = s_i;
                let mut temp_shift = cumulative_shift;

                // Check if we are inside a delete initially
                if temp_s_i < s_ops.len() {
                    let sop = s_ops[temp_s_i];
                    let sop_ins = sop.ins as i64;
                    if sop_ins <= curr && sop.len < 0 {
                        let sop_end = sop_ins - sop.len as i64;
                        let overlap = sop_end.min(del_end) - curr;
                        curr += overlap;
                        temp_shift -= overlap;
                        if sop_end <= del_end {
                            temp_s_i += 1;
                        }
                    }
                }

                while curr < del_end {
                    if temp_s_i >= s_ops.len() {
                        let len = del_end - curr;
                        let ins: InsertPos = (curr + temp_shift)
                            .try_into()
                            .expect("transform delete overflow");
                        let len_i32: Length =
                            len.try_into().expect("transform delete len overflow");
                        Self::push_op(&mut res_ops, Op { ins, len: -len_i32 });
                        break;
                    }

                    let sop = s_ops[temp_s_i];
                    let sop_ins = sop.ins as i64;

                    if sop_ins >= del_end {
                        let len = del_end - curr;
                        let ins: InsertPos = (curr + temp_shift)
                            .try_into()
                            .expect("transform delete overflow");
                        let len_i32: Length =
                            len.try_into().expect("transform delete len overflow");
                        Self::push_op(&mut res_ops, Op { ins, len: -len_i32 });
                        break;
                    }

                    // sop.ins is in [curr, del_end)
                    if sop_ins > curr {
                        let len = sop_ins - curr;
                        let ins: InsertPos = (curr + temp_shift)
                            .try_into()
                            .expect("transform delete overflow");
                        let len_i32: Length =
                            len.try_into().expect("transform delete len overflow");
                        Self::push_op(&mut res_ops, Op { ins, len: -len_i32 });
                        curr = sop_ins;
                    }

                    // Now curr == sop_ins
                    if sop.len > 0 {
                        if shift_on_tie {
                            temp_shift += sop.len as i64;
                        }
                        temp_s_i += 1;
                    } else {
                        let sop_end = sop_ins - sop.len as i64;
                        let overlap = sop_end.min(del_end) - curr;
                        curr += overlap;
                        temp_shift -= overlap;
                        if sop_end <= del_end {
                            temp_s_i += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        OpList {
            ops: res_ops,
            test_op: None,
        }
    }

    fn push_op(ops: &mut Vec<Op>, op: Op) {
        if op.len == 0 {
            return;
        }
        if let Some(last) = ops.last_mut() {
            if op.len > 0 && last.len > 0 {
                if last.ins == op.ins {
                    last.len += op.len;
                    return;
                }
            }
            if op.len < 0 && last.len < 0 {
                let last_end = last.ins as i64 - last.len as i64;
                if last_end == op.ins as i64 {
                    last.len += op.len;
                    return;
                }
            }
        }
        ops.push(op);
    }
}

fn main() {}

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

    #[test]
    fn transform_behaviors() {
        let s = getOpList([(5, 2)]);
        let o = getOpList([(5, 3)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(7, 3)]));

        let s = getOpList([(5, -2), (6, 1)]);
        let o = getOpList([(6, 1)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(6, 1)]));

        let s = getOpList([(5, 3)]);
        let o = getOpList([(5, 2)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(8, 2)]));

        let s = getOpList([(5, -2)]);
        let o = getOpList([(6, 1)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(5, 1)]));

        let s = getOpList([(5, -5)]);
        let o = getOpList([(3, -10)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(3, -5)]));

        let s = getOpList([(5, 2)]);
        let o = getOpList([(5, -2)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([(7, -2)]));

        let s = getOpList([(5, -2)]);
        let o = getOpList([(5, -2)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([]));
    }

    #[test]
    fn apply_transformation_behaviors() {
        let mut s = getOpList([(5, 3)]);
        let t = getOpList([(5, 2)]);
        s.apply_transformation(&t);
        // (5, 2) applied on (5, 3) -> (5, 3) because t inserts at 5, s inserts at 5.
        // s is transformed against t.
        // t.transform(s) with shift_on_tie=false.
        // s at 5. t at 5. t inserts 2. s is NOT shifted.
        // So s remains at 5.
        assert_eq!(s, getOpList([(5, 3)]));

        let mut s = getOpList([(5, 3), (6, 1)]);
        let t = getOpList([(5, 2)]);
        s.apply_transformation(&t);
        // s has (5, 3) and (6, 1).
        // (5, 3) -> (5, 3) (as above)
        // (6, 1) -> (8, 1) (shifted by t's insert of 2)
        assert_eq!(s, getOpList([(5, 3), (8, 1)]));
    }

    #[test]
    fn apply_transformation_complex_cases() {
        // Case 1: Transformer deletes range where self inserts.
        // Self: Insert "ABC" at 5. (5, 3)
        // Trans: Delete 2 chars at 5. (5, -2)
        // Result: Self should still insert at 5, but since the context *before* it didn't change (it's at 5),
        // and the delete is *at* 5.
        // If T deletes (5, -2), it means chars 5 and 6 are gone.
        // S inserts at 5.
        // In the new document, 5 and 6 are gone. The insertion point 5 is now... 5.
        // Wait, if 5 and 6 are deleted, the index 5 still exists (it's the start of the deletion).
        // So S should still be (5, 3).
        let mut s = getOpList([(5, 3)]);
        let t = getOpList([(5, -2)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, 3)]));

        // Case 2: Transformer inserts in middle of self's insert.
        // Self: Insert "ABCD" at 5. (5, 4)
        // Trans: Insert "XY" at 7. (7, 2)
        // This is tricky. Self is a single op (5, 4). It doesn't "contain" 7 in the base document.
        // It inserts at 5.
        // T inserts at 7.
        // 7 is AFTER 5.
        // So T's insert is at index 7 of the BASE document.
        // S inserts at 5 of the BASE document.
        // So S is unaffected by T's insert at 7.
        let mut s = getOpList([(5, 4)]);
        let t = getOpList([(7, 2)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, 4)]));

        // Case 3: Transformer deletes a range that overlaps with self's delete.
        // Self: Delete (5, -3) -> deletes 5, 6, 7.
        // Trans: Delete (6, -3) -> deletes 6, 7, 8.
        // Overlap is 6, 7.
        // S deletes 5, 6, 7.
        // T deletes 6, 7, 8.
        // We want to transform S against T.
        // T has deleted 6, 7, 8.
        // S wants to delete 5, 6, 7.
        // 6 and 7 are already deleted by T.
        // So S only needs to delete 5.
        // 5 is before 6. So 5 is still at 5.
        // Result: S should become (5, -1).
        let mut s = getOpList([(5, -3)]);
        let t = getOpList([(6, -3)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, -1)]));

        // Case 4: Transformer deletes a range that is a subset of self's delete.
        // Self: Delete (5, -5) -> 5, 6, 7, 8, 9.
        // Trans: Delete (6, -2) -> 6, 7.
        // T deletes 6, 7.
        // S wants to delete 5..10.
        // 6, 7 are gone.
        // S needs to delete 5, and 8, 9.
        // In the new document (after T), 6 and 7 are gone.
        // 5 is at 5.
        // 8 becomes 6 (shifted back by 2).
        // 9 becomes 7.
        // So S should delete 5, 6, 7 in the new document?
        // Wait.
        // Original: 0 1 2 3 4 5 6 7 8 9 10
        // T deletes 6, 7.
        // New: 0 1 2 3 4 5 8 9 10
        // Indices: 0 1 2 3 4 5 6 7 8
        // S wanted to delete 5, 6, 7, 8, 9.
        // In New, these correspond to:
        // 5 -> 5
        // 6 -> deleted
        // 7 -> deleted
        // 8 -> 6
        // 9 -> 7
        // So S should delete range [5, 8) in New? i.e. 5, 6, 7.
        // So S becomes (5, -3).
        let mut s = getOpList([(5, -5)]);
        let t = getOpList([(6, -2)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, -3)]));

        // Case 5: Transformer deletes a range that is a superset of self's delete.
        // Self: Delete (6, -2) -> 6, 7.
        // Trans: Delete (5, -5) -> 5, 6, 7, 8, 9.
        // T deletes everything S wanted to delete.
        // S should become empty.
        let mut s = getOpList([(6, -2)]);
        let t = getOpList([(5, -5)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([]));

        // Case 6: Mixed operations.
        // Self: Insert (5, 2), Delete (8, -2).
        // Trans: Delete (4, -2) -> 4, 5. Insert (8, 1).
        //
        // T: Delete 4, 5. Insert 1 at 8.
        //
        // S op 1: Insert (5, 2).
        // T deletes 4, 5.
        // Insertion point 5 is at the end of the deletion range [4, 6).
        // So 5 maps to 4.
        // S op 1 becomes (4, 2).
        //
        // S op 2: Delete (8, -2) -> 8, 9.
        // T deletes 4, 5. Shift is -2.
        // T inserts at 8.
        // S delete starts at 8.
        // 8 in base maps to 8 - 2 = 6.
        // But wait, T inserts at 8 (base).
        // 8 (base) is after 4, 5 (deleted).
        // So 8 (base) becomes 6.
        // T inserts at 8 (base).
        // Since T inserts at 8, and S deletes at 8.
        // S delete is AT 8. T insert is AT 8.
        // Does T insert happen before or after S delete?
        // T is the transformer. We are transforming S against T.
        // T's insert at 8 means there is new content at 8.
        // S wanted to delete 8, 9 (original).
        // S should NOT delete the new content inserted by T.
        // So S should still delete the original 8, 9.
        // Original 8 maps to 6 (due to T's delete of 4,5).
        // T's insert is at 8 (original).
        // Wait, T's insert is at 8.
        // If T inserts at 8, it shifts subsequent characters.
        // But S's delete is AT 8.
        // Does S delete the inserted char? No.
        // Does S delete start before or after the inserted char?
        // Usually, if I delete at X, and you insert at X.
        // Your insert shifts my delete?
        // If I delete [8, 10), and you insert at 8.
        // The content I wanted to delete is now at [8+len, 10+len).
        // So S should be shifted by T's insert.
        // T inserts 1 at 8.
        // So S delete (originally at 8) should now be at 8 + 1 = 9?
        // Let's trace carefully.
        // Base: 0 1 2 3 4 5 6 7 8 9 10
        // T: Delete 4, 5. Insert 'X' at 8.
        // Step 1 (Delete 4, 5): 0 1 2 3 6 7 8 9 10. (Length reduced by 2).
        // Indices map: 0->0, ..., 3->3, 6->4, 7->5, 8->6, 9->7.
        // Step 2 (Insert 'X' at 8):
        // Wait, T is a list of ops. They are applied sequentially on Base.
        // T op 1: (4, -2).
        // T op 2: (8, 1).
        // Note: T op 2 position (8) is in the coordinate system AFTER T op 1.
        // After T op 1 (delete 4, 5), the doc is smaller.
        // If T op 2 is (8, 1), it means insert at index 8 of the INTERMEDIATE doc.
        // Intermediate doc: 0 1 2 3 6 7 8 9 10.
        // Index 8 corresponds to...
        // 0, 1, 2, 3, 4(was 6), 5(was 7), 6(was 8), 7(was 9), 8(was 10).
        // So T inserts at old 10?
        //
        // Let's assume the test setup implies T's ops are sequential.
        //
        // S op 1: Insert (5, 2).
        // Target 5.
        // T op 1 (4, -2): Deletes 4, 5. 5 is inside/at end of delete.
        // 5 maps to 4.
        // T op 2 (8, 1): Insert at 8 (intermediate).
        // S op 1 is at 4 (intermediate). 4 < 8.
        // So S op 1 remains at 4.
        // Result S op 1: (4, 2).
        //
        // S op 2: Delete (8, -2) -> 8, 9 (base).
        // Target 8.
        // T op 1 (4, -2): Deletes 4, 5.
        // 8 is > 5. Shift by -2.
        // 8 maps to 6.
        // T op 2 (8, 1): Insert at 8 (intermediate).
        // S op 2 is at 6 (intermediate).
        // 6 < 8.
        // So S op 2 is unaffected by T op 2.
        // Result S op 2: (6, -2).
        //
        // So expected: [(4, 2), (6, -2)].

        let mut s = getOpList([(5, 2), (8, -2)]);
        let t = getOpList([(4, -2), (8, 1)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(4, 2), (6, -2)]));
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

    /// Helper: associates each op with its base anchor to simplify reference comparisons.
    fn ops_with_base(seq: &OpList) -> Vec<(i64, Op)> {
        let mut result = Vec::new();
        let mut base_cursor: i64 = 0;
        let mut doc_cursor: i64 = 0;

        for range in &seq.ops {
            if range.len == 0 {
                continue;
            }

            let range_base = i64::from(range.ins);
            if range_base > base_cursor {
                doc_cursor += range_base - base_cursor;
                base_cursor = range_base;
            }

            if range.len > 0 {
                let ins: InsertPos = doc_cursor.try_into().expect("insert cursor overflow");
                result.push((
                    range_base,
                    Op {
                        ins,
                        len: range.len,
                    },
                ));
                doc_cursor += i64::from(range.len);
            } else {
                let delete_len = -i64::from(range.len);
                let delete_start = doc_cursor;
                let ins: InsertPos = (delete_start + delete_len)
                    .try_into()
                    .expect("delete cursor overflow");
                let len: Length = delete_len.try_into().expect("delete len overflow");
                result.push((range_base, Op { ins, len: -len }));
                base_cursor += delete_len;
            }
        }

        result
    }

    /// Reference implementation used to validate `backwards_apply`.
    fn backwards_apply_reference(current: &OpList, prior: &OpList) -> OpList {
        let mut baseline = prior.clone();
        let ops = ops_with_base(current);
        let mut shift_all: i64 = 0;
        let mut shift_deletes: i64 = 0;
        let mut prior_ops_iter = prior.ops.iter().peekable();

        for (base, mut op) in ops {
            // Process prior operations that affect this operation's base position
            while let Some(prior_op) = prior_ops_iter.peek() {
                let prior_base = i64::from(prior_op.ins);
                let effective_base = if prior_op.len > 0 {
                    prior_base
                } else {
                    // For deletes, the effective base is at the end of the deleted range
                    prior_base + i64::from(-prior_op.len)
                };

                if effective_base <= base {
                    let prior_op = *prior_ops_iter.next().unwrap();
                    if prior_op.len > 0 {
                        shift_all += i64::from(prior_op.len);
                    } else {
                        let delete_len = -i64::from(prior_op.len);
                        shift_all += -delete_len;
                        shift_deletes += -delete_len;
                    }
                } else {
                    break;
                }
            }

            if op.len == 0 {
                continue;
            } else if op.len > 0 {
                let adjusted = i64::from(op.ins) + shift_deletes;
                let ins: InsertPos = adjusted.try_into().expect("insert cursor overflow");
                OpList::apply_insert(&mut baseline.ops, ins, op.len);
            } else {
                let start = i64::from(op.ins + op.len) + shift_all;
                let start_pos: InsertPos = start.try_into().expect("delete start overflow");
                let len = -op.len;
                OpList::apply_delete(&mut baseline.ops, start_pos, len);
            }
        }

        baseline.test_op = None;
        baseline
    }

    /// Spot-checks simple backwards-apply scenarios against the reference version.
    #[test]
    fn backwards_apply_handles_simple_examples() {
        let current = getOpList([(3, -2)]);
        let prior = getOpList([(2, -1)]);
        let expected = getOpList([(2, -3)]);
        assert_eq!(current.backwards_apply(&prior), expected);
        assert_eq!(
            current.backwards_apply(&prior),
            backwards_apply_reference(&current, &prior)
        );

        let current = getOpList([(3, 1)]);
        let prior = getOpList([(2, 1)]);
        let expected = getOpList([(2, 2)]);
        assert_eq!(current.backwards_apply(&prior), expected);
        assert_eq!(
            current.backwards_apply(&prior),
            backwards_apply_reference(&current, &prior)
        );
    }

    /// Exhaustively compares several composed cases with the reference implementation.
    #[test]
    fn backwards_apply_matches_reference_implementation() {
        let cases = vec![
            (getOpList([(3, -2)]), getOpList([(2, -1)])),
            (getOpList([(3, 1)]), getOpList([(2, 1)])),
            (getOpList([(5, -1), (5, 1)]), getOpList([(4, 1)])),
            (
                getOpList([(2, -3), (2, 2), (7, -1)]),
                getOpList([(6, 2), (9, -2)]),
            ),
            (
                getOpList([(1, 2), (4, -1), (4, 1)]),
                getOpList([(3, -1), (5, 1), (7, -2)]),
            ),
        ];

        for (current, prior) in cases {
            let expected = backwards_apply_reference(&current, &prior);
            assert_eq!(current.backwards_apply(&prior), expected);
        }
    }

    /// Regression suite covering incremental state building and edge cases.
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
        let test_vec: OpList = getOpListforTesting([(5, -1)], [(7, 1)]);
        let expected_result = getOpList([(5, -1), (8, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-67890...
        // After new applied (+) = 12345-6+7890...
        let test_vec: OpList = getOpListforTesting([(5, 1)], [(7, 1)]);
        let expected_result = getOpList([(5, 1), (6, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Test cases for: 1234567 -> 123456-7= -> 12345-=

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345+-=890...
        let test_vec: OpList = getOpListforTesting([(5, -2), (6, 1), (7, 1)], [(5, 1)]);
        let expected_result = getOpList([(5, 1), (5, -2), (6, 1), (7, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-+=890...
        let test_vec: OpList = getOpListforTesting([(5, -2), (6, 1), (7, 1)], [(6, 1)]);
        let expected_result = getOpList([(5, -2), (6, 2), (7, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-=8+90...
        let test_vec: OpList = getOpListforTesting([(5, -2), (6, 1), (7, 1)], [(8, 1)]);
        let expected_result = getOpList([(5, -2), (6, 1), (7, 1), (8, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Test cases for 1-2-35-

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-+35-67890...
        let test_vec: OpList = getOpListforTesting([(1, 1), (2, 1), (3, -1), (5, 1)], [(4, 1)]);
        let expected_result = getOpList([(1, 1), (2, 2), (3, -1), (5, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-3+5-67890...
        let test_vec: OpList = getOpListforTesting([(1, 1), (2, 1), (3, -1), (5, 1)], [(5, 1)]);
        let expected_result = getOpList([(1, 1), (2, 1), (3, 1), (3, -1), (5, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35+-67890...
        let test_vec: OpList = getOpListforTesting([(1, 1), (2, 1), (3, -1), (5, 1)], [(6, 1)]);
        let expected_result = getOpList([(1, 1), (2, 1), (3, -1), (5, 2)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35-6+7890...
        let test_vec: OpList = getOpListforTesting([(1, 1), (2, 1), (3, -1), (5, 1)], [(8, 1)]);
        let expected_result = getOpList([(1, 1), (2, 1), (3, -1), (5, 1), (6, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Test for 4,-4 and stuff to check for first elements.

        // Test for delete RLE

        // Base = 1234567890...
        // Pre existing = 12345790...
        // After new applied = 1234590...
        let test_vec: OpList = getOpListforTesting([(5, -1), (7, -1)], [(6, -1)]);
        let expected_result = getOpList([(5, -3)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-=~90...
        let mut test_vec: OpList =
            getOpListforTesting([(3, -3), (4, 1), (5, 1), (6, 1), (7, -1)], [(7, -1)]);
        let expected_result = getOpList([(3, -5), (4, 1), (5, 1), (6, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-90...
        let mut test_vec: OpList =
            getOpListforTesting([(3, -3), (4, 1), (5, 1), (6, 1), (7, -1)], [(7, -3)]);
        let expected_result = getOpList([(3, -5), (4, 1)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1234567-90...
        // After new applied = 12345690...
        let mut test_vec: OpList = getOpListforTesting([(7, 1), (7, -1)], [(8, -2)]);
        let expected_result = getOpList([(6, -2)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 14678-=~90...
        // Expected = 14678-=~90...
        let mut test_vec: OpList = getOpList([(5, -1), (3, -1), (6, 3), (2, -1)]); // hard to understand
        let expected_result = getOpList([(1, -2), (4, -1), (8, 3)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 127890...
        // Expected = 127890...
        let mut test_vec: OpList = getOpList([(5, -2), (4, -2)]); // 1234567 -> 12367 -> 127; Testing for delete RLE within delete RLE
        let expected_result = getOpList([(2, -4)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Basic test
        let test_vec: OpList = getOpListforTesting([(5, 5)], [(7, -2)]);
        let expected_result = getOpList([(5, 3)]); // Should be "heo" at position 5
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // // Could be useful
        // // -11-2---
        // let test_vec: OpList = getOpListforTesting([(1,1),(1,1)], [(4,1)]);
        // let expected_result = getOpList([(1,1),(1,1),(2,1)]);
        // assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
    }
}
