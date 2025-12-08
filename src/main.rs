#![allow(warnings)]

// Type aliases for better readability
type InsertPos = i32;
type Length = i32;

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Insert { ins: InsertPos, content: String },
    Delete { ins: InsertPos, len: Length },
}

impl Op {
    pub fn len(&self) -> Length {
        match self {
            Op::Insert { content, .. } => content.len() as Length,
            Op::Delete { len, .. } => *len,
        }
    }

    pub fn ins(&self) -> InsertPos {
        match self {
            Op::Insert { ins, .. } => *ins,
            Op::Delete { ins, .. } => *ins,
        }
    }

    pub fn set_ins(&mut self, new_ins: InsertPos) {
        match self {
            Op::Insert { ins, .. } => *ins = new_ins,
            Op::Delete { ins, .. } => *ins = new_ins,
        }
    }

    pub fn append(&mut self, other: Op) {
        match (self, other) {
            (Op::Insert { content: c1, .. }, Op::Insert { content: c2, .. }) => {
                c1.push_str(&c2);
            }
            (Op::Delete { len: l1, .. }, Op::Delete { len: l2, .. }) => {
                *l1 += l2;
            }
            _ => panic!("Cannot append mismatched ops"),
        }
    }

    pub fn prepend(&mut self, other: Op) {
        match (self, other) {
            (Op::Insert { content: c1, .. }, Op::Insert { content: c2, .. }) => {
                c1.insert_str(0, &c2);
            }
            _ => panic!("Cannot prepend mismatched ops"),
        }
    }

    pub fn extend_delete(&mut self, delta: Length) {
        if let Op::Delete { len, .. } = self {
            *len += delta;
        } else {
            panic!("Cannot extend_delete on Insert");
        }
    }

    pub fn remove_range(&mut self, start: usize, end: usize) {
        match self {
            Op::Insert { content, .. } => {
                content.replace_range(start..end, "");
            }
            _ => panic!("Cannot remove_range on Delete"),
        }
    }
    pub fn insert_at(&mut self, offset: usize, new_content: &str) {
        match self {
            Op::Insert { content, .. } => {
                content.insert_str(offset, new_content);
            }
            _ => panic!("Cannot insert_at on Delete"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct OpList {
    /// List of operations to be applied
    ops: Vec<Op>,
    /// Test data for debugging (should be removed in production)
    test_op: Option<Vec<Op>>,
}

// set DTRACE = "C:\Users\dex\PC-Developement\blondie\target\release\blondie_dtrace.exe"
// https://github.com/nico-abram/blondie

trait IntoOp {
    fn into_op(self) -> Op;
}

impl IntoOp for (InsertPos, Length) {
    fn into_op(self) -> Op {
        let (ins, len) = self;
        if len >= 0 {
            panic!("Positive length in (InsertPos, Length) is not allowed. Use (InsertPos, &str) or TestOp instead.");
        } else {
            Op::Delete { ins, len }
        }
    }
}

impl IntoOp for (InsertPos, &str) {
    fn into_op(self) -> Op {
        let (ins, content) = self;
        Op::Insert {
            ins,
            content: content.to_string(),
        }
    }
}

impl IntoOp for Op {
    fn into_op(self) -> Op {
        self
    }
}

#[derive(Copy, Clone)]
enum TestOp {
    Ins(InsertPos, &'static str),
    Del(InsertPos, Length),
}

impl IntoOp for TestOp {
    fn into_op(self) -> Op {
        match self {
            TestOp::Ins(ins, content) => Op::Insert {
                ins,
                content: content.to_string(),
            },
            TestOp::Del(ins, len) => Op::Delete { ins, len },
        }
    }
}

/// Builds an `OpList` from a fixed-size array of (position, length) tuples while clearing any testing state.
fn getOpList<T: IntoOp, const N: usize>(list: [T; N]) -> OpList {
    OpList {
        ops: list.into_iter().map(|x| x.into_op()).collect(),
        test_op: None,
    }
}

/// Builds an `OpList` from a runtime `Vec` of (position, length) tuples while clearing any testing state.
fn getOpListbyVec<T: IntoOp>(list: Vec<T>) -> OpList {
    OpList {
        ops: list.into_iter().map(|x| x.into_op()).collect(),
        test_op: None,
    }
}

/// Builds an `OpList` and seeds it with a pre-existing sequential list for testing.
fn getOpListforTesting<T: IntoOp + Copy, U: IntoOp, const N: usize, const M: usize>(
    pre_existing_range_list: [T; N],
    oplist: [U; M],
) -> OpList {
    OpList {
        ops: oplist.into_iter().map(|x| x.into_op()).collect(),
        test_op: Some(
            pre_existing_range_list
                .into_iter()
                .map(|x| x.into_op())
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
            .map(|ops| ops.clone())
            .unwrap_or_else(Vec::new);

        for op in &self.ops {
            if op.len() == 0 {
                continue;
            }

            if op.len() > 0 {
                match op {
                    Op::Insert { ins, content } => {
                        Self::apply_insert(&mut ranges, *ins, content.clone());
                    }
                    _ => unreachable!(),
                }
            } else {
                let start = op.ins() + op.len();
                let len = -op.len();
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
            if range.len() == 0 {
                continue;
            }

            let range_base = i64::from(range.ins());
            if range_base > base_cursor {
                let advance = range_base - base_cursor;
                doc_cursor += advance;
                base_cursor = range_base;
            }

            // Process prior operations that affect this range's base position
            while let Some(prior_op) = prior_ops_iter.peek() {
                let prior_base = i64::from(prior_op.ins());
                let effective_base = if prior_op.len() > 0 {
                    prior_base
                } else {
                    // For deletes, the effective base is at the end of the deleted range
                    prior_base + i64::from(-prior_op.len())
                };

                if effective_base <= range_base {
                    let prior_op = prior_ops_iter.next().unwrap();
                    if prior_op.len() > 0 {
                        cumulative_shift_all += i64::from(prior_op.len());
                    } else {
                        let delete_len = -i64::from(prior_op.len());
                        cumulative_shift_all += -delete_len;
                        cumulative_shift_deletes += -delete_len;
                    }
                } else {
                    break;
                }
            }

            let adjusted_cursor = if range.len() > 0 {
                doc_cursor + cumulative_shift_deletes
            } else {
                doc_cursor + cumulative_shift_deletes
            };

            if range.len() > 0 {
                let ins: InsertPos = adjusted_cursor.try_into().expect("insert cursor overflow");
                match range {
                    Op::Insert { content, .. } => {
                        Self::apply_insert(ranges, ins, content.clone());
                    }
                    _ => unreachable!(),
                }
                doc_cursor += i64::from(range.len());
            } else {
                let delete_len = -i64::from(range.len());
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
            let range = self.ops[read_idx].clone();
            if range.len() == 0 {
                continue;
            }

            let range_len = range.len();
            let range_base = i64::from(range.ins());
            if range_base > base_cursor {
                let advance = range_base - base_cursor;
                doc_cursor += advance;
                base_cursor = range_base;
            }

            if range_len > 0 {
                let ins: InsertPos = doc_cursor.try_into().expect("insert cursor overflow");
                match range {
                    Op::Insert { content, .. } => {
                        Self::write_op(&mut self.ops, write_idx, Op::Insert { ins, content });
                    }
                    _ => unreachable!(),
                }
                write_idx += 1;
                doc_cursor += i64::from(range_len);
            } else {
                let delete_len = -i64::from(range_len);
                let delete_start = doc_cursor;
                let ins: InsertPos = (delete_start + delete_len)
                    .try_into()
                    .expect("delete cursor overflow");
                let len: Length = delete_len.try_into().expect("delete len overflow");
                Self::write_op(&mut self.ops, write_idx, Op::Delete { ins, len: -len });
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
            if op.len() == 0 {
                continue;
            } else if op.len() > 0 {
                Self::merge_insert(&mut self.ops, op.clone());
            } else {
                Self::merge_delete(&mut self.ops, op.clone());
            }
        }
    }

    /// Merges a positive-length operation into an ordered list, combining adjacent inserts at the same base.
    fn merge_insert(ranges: &mut Vec<Op>, op: Op) {
        debug_assert!(op.len() > 0);

        let mut idx = 0;
        while idx < ranges.len() && ranges[idx].ins() < op.ins() {
            idx += 1;
        }

        while idx < ranges.len() && ranges[idx].ins() == op.ins() {
            if ranges[idx].len() > 0 {
                ranges[idx].append(op);
                return;
            }
            idx += 1;
        }

        ranges.insert(idx, op);
    }

    /// Merges a delete operation into an ordered list, coalescing overlapping delete spans.
    fn merge_delete(ranges: &mut Vec<Op>, op: Op) {
        debug_assert!(op.len() < 0);

        let mut delete_start = op.ins() as i64;
        let mut delete_end = Self::delete_end(&op) as i64;
        let original_len = ranges.len();
        let mut read_idx: usize = 0;
        let mut write_idx: usize = 0;
        let mut inserted = false;
        let mut inserted_idx: Option<usize> = None;

        while read_idx < original_len {
            let current = ranges[read_idx].clone();
            read_idx += 1;

            if current.len() < 0 {
                let current_start = current.ins() as i64;
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

            let base = current.ins() as i64;
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
    fn apply_insert(ranges: &mut Vec<Op>, pos: InsertPos, content: String) {
        let len = content.len() as Length;
        if len <= 0 {
            return;
        }

        match Self::locate_position(ranges, pos, LocateBias::PreferOutsideInsert) {
            PositionRef::Insert { index, offset } => {
                ranges[index].insert_at(offset as usize, &content);
            }
            PositionRef::Base { base, index } => {
                Self::insert_positive(ranges, index, base, content);
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
            let mut current = ranges[read_idx].clone();
            read_idx += 1;
            let next_ins = current.ins() as i64;

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

            if current.len() < 0 {
                base_cursor += i64::from(-current.len());
                Self::emit_delete_op(
                    ranges,
                    &mut write_idx,
                    &mut last_delete_idx,
                    DeleteEmit::Existing(current),
                );
            } else if current.len() > 0 {
                let seg_len = current.len() as i64;
                let (overlap_len, overlap_start) =
                    Self::segment_overlap(doc_cursor, seg_len, delete_cursor, delete_end);
                if overlap_len > 0 {
                    let start_offset = (overlap_start - doc_cursor) as usize;
                    let end_offset = start_offset + overlap_len as usize;
                    current.remove_range(start_offset, end_offset);
                    delete_cursor += overlap_len;
                }

                if current.len() > 0 {
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
    fn insert_positive(ranges: &mut Vec<Op>, idx: usize, base: InsertPos, content: String) {
        let len = content.len() as Length;
        if len <= 0 {
            return;
        }

        let insert_idx = idx;

        if insert_idx > 0 {
            if let Some(prev) = ranges.get_mut(insert_idx - 1) {
                if prev.len() > 0 && prev.ins() == base {
                    prev.append(Op::Insert {
                        ins: base,
                        content: content.clone(),
                    });
                    return;
                }
            }
        }

        if insert_idx < ranges.len() {
            if ranges[insert_idx].len() > 0 && ranges[insert_idx].ins() == base {
                // If we insert at the same base as an existing insert, we prepend.
                // Example: Existing "ABC" at 1. Insert "A" at 1. Result "AABC".
                ranges[insert_idx].prepend(Op::Insert { ins: base, content });
                return;
            }
        }

        ranges.insert(insert_idx, Op::Insert { ins: base, content });
    }

    /// Finds where a given document position lives within the range list, honoring the provided bias.
    fn locate_position(ranges: &[Op], pos: InsertPos, bias: LocateBias) -> PositionRef {
        let mut base_cursor: i64 = 0;
        let mut doc_cursor: i64 = 0;
        let target = pos as i64;

        for (index, range) in ranges.iter().enumerate() {
            let range_base = range.ins() as i64;
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

            if range.len() < 0 {
                if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
                    return PositionRef::Base {
                        base: range.ins(),
                        index,
                    };
                }
                base_cursor += i64::from(-range.len());
                continue;
            } else {
                let insert_len = range.len() as i64;
                if matches!(bias, LocateBias::PreferOutsideInsert) && target == doc_cursor {
                    return PositionRef::Base {
                        base: range.ins(),
                        index,
                    };
                }
                if matches!(bias, LocateBias::PreferOutsideInsert)
                    && target == doc_cursor + insert_len
                {
                    return PositionRef::Insert {
                        index,
                        offset: range.len(),
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
                debug_assert!(op.len() <= 0);
                if op.len() == 0 {
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
                Op::Delete { ins, len: -len_i32 }
            }
        };

        if let Some(idx) = *last_delete_idx {
            if Self::delete_end(&ranges[idx]) == delete_op.ins() {
                ranges[idx].extend_delete(delete_op.len());
                return;
            }
        }

        Self::write_op(ranges, *write_idx, delete_op);
        *last_delete_idx = Some(*write_idx);
        *write_idx += 1;
    }

    /// Computes the exclusive end position of a delete operation.
    fn delete_end(op: &Op) -> InsertPos {
        debug_assert!(op.len() < 0);
        let end = op.ins() as i64 - op.len() as i64;
        end as InsertPos
    }

    /// Creates a delete operation spanning from `start` to `end` in base coordinates.
    fn delete_span(start: i64, end: i64) -> Op {
        debug_assert!(end > start);
        let ins: InsertPos = start.try_into().expect("delete base overflow");
        let len_i64 = end - start;
        let len: Length = len_i64.try_into().expect("delete len overflow");
        Op::Delete { ins, len: -len }
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
            let target = op.ins() as i64;

            // Advance s_i to target
            while s_i < s_ops.len() {
                let sop = &s_ops[s_i];
                let sop_ins = sop.ins() as i64;
                if sop_ins > target {
                    break;
                }

                if sop.len() < 0 {
                    let sop_end = sop_ins - sop.len() as i64;
                    if sop_end > target {
                        // Overlaps target. Don't consume.
                        break;
                    }
                    // Fully before target
                    cumulative_shift += sop.len() as i64;
                    s_i += 1;
                } else {
                    // Insert
                    if sop_ins == target && !shift_on_tie {
                        break;
                    }
                    cumulative_shift += sop.len() as i64;
                    s_i += 1;
                }
            }

            if op.len() > 0 {
                let mut mapped_pos = target + cumulative_shift;
                let mut temp_s_i = s_i;

                while temp_s_i < s_ops.len() {
                    let sop = &s_ops[temp_s_i];
                    let sop_ins = sop.ins() as i64;
                    if sop_ins > target {
                        break;
                    }

                    if sop.len() > 0 {
                        if sop_ins == target && !shift_on_tie {
                            // Don't shift for inserts at target if !shift_on_tie
                        } else {
                            mapped_pos += sop.len() as i64;
                        }
                    } else {
                        let sop_end = sop_ins - sop.len() as i64;
                        if sop_ins <= target && target < sop_end {
                            mapped_pos -= target - sop_ins;
                        }
                    }
                    temp_s_i += 1;
                }

                let ins: InsertPos = mapped_pos.try_into().expect("transform insert overflow");
                match op {
                    Op::Insert { content, .. } => {
                        Self::push_op(
                            &mut res_ops,
                            Op::Insert {
                                ins,
                                content: content.clone(),
                            },
                        );
                    }
                    _ => unreachable!(),
                }
            } else {
                let del_len = -op.len() as i64;
                let del_end = target + del_len;
                let mut curr = target;
                let mut temp_s_i = s_i;
                let mut temp_shift = cumulative_shift;

                // Check if we are inside a delete initially
                if temp_s_i < s_ops.len() {
                    let sop = &s_ops[temp_s_i];
                    let sop_ins = sop.ins() as i64;
                    if sop_ins <= curr && sop.len() < 0 {
                        let sop_end = sop_ins - sop.len() as i64;
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
                        Self::push_op(&mut res_ops, Op::Delete { ins, len: -len_i32 });
                        break;
                    }

                    let sop = &s_ops[temp_s_i];
                    let sop_ins = sop.ins() as i64;

                    if sop_ins >= del_end {
                        let len = del_end - curr;
                        let ins: InsertPos = (curr + temp_shift)
                            .try_into()
                            .expect("transform delete overflow");
                        let len_i32: Length =
                            len.try_into().expect("transform delete len overflow");
                        Self::push_op(&mut res_ops, Op::Delete { ins, len: -len_i32 });
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
                        Self::push_op(&mut res_ops, Op::Delete { ins, len: -len_i32 });
                        curr = sop_ins;
                    }

                    // Now curr == sop_ins
                    if sop.len() > 0 {
                        if shift_on_tie {
                            temp_shift += sop.len() as i64;
                        }
                        temp_s_i += 1;
                    } else {
                        let sop_end = sop_ins - sop.len() as i64;
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
        if op.len() == 0 {
            return;
        }
        if let Some(last) = ops.last_mut() {
            if op.len() > 0 && last.len() > 0 {
                if last.ins() == op.ins() {
                    last.append(op);
                    return;
                }
            }
            if op.len() < 0 && last.len() < 0 {
                let last_end = last.ins() as i64 - last.len() as i64;
                if last_end == op.ins() as i64 {
                    last.extend_delete(op.len());
                    return;
                }
            }
        }
        ops.push(op);
    }
}

fn main() {}

#[derive(Clone, Debug)]
struct GraphNode {
    op: OpList,
    parents: Vec<usize>,
    children: Vec<usize>,
}

struct Graph {
    nodes: std::collections::HashMap<usize, GraphNode>,
    root: usize,
    frontier: Vec<usize>,
}

impl Graph {
    fn new(root: usize, root_op: OpList) -> Self {
        let mut nodes = std::collections::HashMap::new();
        nodes.insert(
            root,
            GraphNode {
                op: root_op,
                parents: vec![],
                children: vec![],
            },
        );
        Graph {
            nodes,
            root,
            frontier: vec![root],
        }
    }

    fn add_node(&mut self, id: usize, op: OpList, parents: Vec<usize>) {
        // Update parents to point to this child
        for &parent_id in &parents {
            if let Some(parent) = self.nodes.get_mut(&parent_id) {
                parent.children.push(id);
            }
        }

        self.nodes.insert(
            id,
            GraphNode {
                op,
                parents,
                children: vec![],
            },
        );
    }

    fn merge_graph(&self) -> OpList {
        let mut visited = std::collections::HashSet::new();
        self.walk(self.root, &mut visited)
    }

    fn walk(&self, node_id: usize, visited: &mut std::collections::HashSet<usize>) -> OpList {
        if visited.contains(&node_id) {
            return OpList {
                ops: vec![],
                test_op: None,
            };
        }
        visited.insert(node_id);

        let node = self.nodes.get(&node_id).expect("Node not found");
        let node_seq = node.op.from_oplist_to_sequential_list();

        if node.children.is_empty() {
            return node_seq;
        }

        let mut sorted_children = node.children.clone();
        sorted_children.sort();

        let mut child_results = Vec::new();
        for child_id in sorted_children {
            child_results.push(self.walk(child_id, visited));
        }

        let mut merged_children = child_results[0].clone();
        for other in &child_results[1..] {
            merged_children.merge_sequential_list(other);
        }

        merged_children.backwards_apply(&node_seq)
    }
}

fn oplist_to_string(oplist: &OpList) -> String {
    let mut res = String::new();
    for op in &oplist.ops {
        if let Op::Insert { content, .. } = op {
            res.push_str(content);
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies merging sequential lists coalesce correctly for mixed insert/delete cases.
    #[test]
    fn merge_sequential_list_behaviors() {
        // Provided case: inserts combine and new positions are appended.
        let mut existing = getOpList([TestOp::Ins(5, "AB"), TestOp::Ins(10, "C")]);
        let additions = getOpList([TestOp::Ins(5, "DEF"), TestOp::Ins(7, "G")]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([
            TestOp::Ins(5, "ABDEF"),
            TestOp::Ins(7, "G"),
            TestOp::Ins(10, "C"),
        ]);
        assert_eq!(existing, expected);

        // Provided case (also covers previous delete-span test): deletes union together.
        let mut existing = getOpList([(5, -1)]);
        let additions = getOpList([(6, -1)]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([(5, -2)]);
        assert_eq!(existing, expected);

        // Provided case: delete spans across multiple segments.
        let mut existing = getOpList([TestOp::Del(3, -1), TestOp::Ins(3, "A"), TestOp::Del(6, -1)]);
        let additions = getOpList([(4, -2)]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([TestOp::Del(3, -4), TestOp::Ins(3, "A")]);
        assert_eq!(existing, expected);

        // Provided case: delete must land before positive insert at same base.
        let mut existing = getOpList([(5, "A")]);
        let additions = getOpList([(5, -2)]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([TestOp::Del(5, -2), TestOp::Ins(5, "A")]);
        assert_eq!(existing, expected);

        // Existing case: inserts at identical base sum their lengths.
        let mut existing = getOpList([(5, "AB")]);
        let additions = getOpList([(5, "CDE")]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([(5, "ABCDE")]);
        assert_eq!(existing, expected);

        // Existing case: mixed operations keep final ordering and RLE.
        let mut existing = getOpList([TestOp::Del(5, -2), TestOp::Ins(5, "A")]);
        let additions = getOpList([TestOp::Ins(5, "B"), TestOp::Del(6, -1)]);
        existing.merge_sequential_list(&additions);
        let expected = getOpList([TestOp::Del(5, -2), TestOp::Ins(5, "AB")]);
        assert_eq!(existing, expected);
    }

    #[test]
    fn transform_behaviors() {
        let s = getOpList([TestOp::Ins(5, "AB")]);
        let o = getOpList([TestOp::Ins(5, "CDE")]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Ins(7, "CDE")]));

        let s = getOpList([TestOp::Del(5, -2), TestOp::Ins(6, "G")]);
        let o = getOpList([TestOp::Ins(6, "F")]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Ins(6, "F")]));

        let s = getOpList([TestOp::Ins(5, "ABC")]);
        let o = getOpList([TestOp::Ins(5, "DE")]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Ins(8, "DE")]));

        let s = getOpList([TestOp::Del(5, -2)]);
        let o = getOpList([TestOp::Ins(6, "F")]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Ins(5, "F")]));

        let s = getOpList([TestOp::Del(5, -5)]);
        let o = getOpList([TestOp::Del(3, -10)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Del(3, -5)]));

        let s = getOpList([TestOp::Ins(5, "AB")]);
        let o = getOpList([TestOp::Del(5, -2)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList([TestOp::Del(7, -2)]));

        let s = getOpList([TestOp::Del(5, -2)]);
        let o = getOpList([TestOp::Del(5, -2)]);
        let res = s.transform(&o);
        assert_eq!(res, getOpList::<(InsertPos, Length), 0>([]));
    }

    #[test]
    fn apply_transformation_behaviors() {
        let mut s = getOpList([(5, "ABC")]);
        let t = getOpList([(5, "XY")]);
        s.apply_transformation(&t);
        // (5, 2) applied on (5, 3) -> (5, 3) because t inserts at 5, s inserts at 5.
        // s is transformed against t.
        // t.transform(s) with shift_on_tie=false.
        // s at 5. t at 5. t inserts 2. s is NOT shifted.
        // So s remains at 5.
        assert_eq!(s, getOpList([(5, "ABC")]));

        let mut s = getOpList([TestOp::Ins(5, "ABC"), TestOp::Ins(6, "D")]);
        let t = getOpList([(5, "XY")]);
        s.apply_transformation(&t);
        // s has (5, 3) and (6, 1).
        // (5, 3) -> (5, 3) (as above)
        // (6, 1) -> (8, 1) (shifted by t's insert of 2)
        assert_eq!(s, getOpList([TestOp::Ins(5, "ABC"), TestOp::Ins(8, "D")]));
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
        let mut s = getOpList([(5, "ABC")]);
        let t = getOpList([(5, -2)]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, "ABC")]));

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
        let mut s = getOpList([(5, "ABCD")]);
        let t = getOpList([(7, "XY")]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([(5, "ABCD")]));

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
        assert_eq!(s, getOpList::<(InsertPos, Length), 0>([]));

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

        let mut s = getOpList([TestOp::Ins(5, "AB"), TestOp::Del(8, -2)]);
        let t = getOpList([TestOp::Del(4, -2), TestOp::Ins(8, "X")]);
        s.apply_transformation(&t);
        assert_eq!(s, getOpList([TestOp::Ins(4, "AB"), TestOp::Del(6, -2)]));
    }

    /// Ensures sequential lists are converted back into op lists with expected coordinates.
    #[test]
    fn sequential_list_to_oplist_emits_expected_ops() {
        let mut sequential = getOpList([TestOp::Del(5, -1), TestOp::Ins(5, "A")]);
        sequential.from_sequential_list_to_oplist();
        let expected = getOpList([TestOp::Del(6, -1), TestOp::Ins(5, "A")]);
        assert_eq!(sequential, expected);

        let mut sequential = getOpList([(2, -4)]);
        sequential.from_sequential_list_to_oplist();
        let expected = getOpList([(6, -4)]);
        assert_eq!(sequential, expected);

        let mut sequential = getOpList([TestOp::Del(2, -3), TestOp::Ins(2, "A")]);
        sequential.from_sequential_list_to_oplist();
        let expected = getOpList([TestOp::Del(5, -3), TestOp::Ins(2, "A")]);
        assert_eq!(sequential, expected);

        let mut sequential = getOpList([TestOp::Del(3, -1), TestOp::Ins(5, "AB")]);
        sequential.from_sequential_list_to_oplist();
        let expected = getOpList([TestOp::Del(4, -1), TestOp::Ins(4, "AB")]);
        assert_eq!(sequential, expected);
    }

    /// Confirms round-trip conversions preserve simple states.
    #[test]
    fn sequential_list_preserves_simple_states() {
        let mut sequential = getOpList([TestOp::Ins(5, "AB"), TestOp::Ins(7, "C")]);
        let expected_state = sequential.clone();
        sequential.from_sequential_list_to_oplist();
        assert_eq!(sequential.from_oplist_to_sequential_list(), expected_state);

        let mut sequential = getOpList([(2, -4)]);
        let expected_state = sequential.clone();
        sequential.from_sequential_list_to_oplist();
        assert_eq!(sequential.from_oplist_to_sequential_list(), expected_state);
    }

    /// Helper: associates each sequential range with the original base anchor so we can
    /// re-run or render it in terms of `Op` coordinates.
    /// Example: sequential `[Insert(base=5,"AB")]` becomes `(5, Insert { ins: doc_pos, ... })`
    /// and `[Delete(base=3,len=-2)]` becomes `(3, Delete { ins: base_end, ... })`.
    fn ops_with_base(seq: &OpList) -> Vec<(i64, Op)> {
        let mut result = Vec::new();
        let mut base_cursor: i64 = 0;
        let mut doc_cursor: i64 = 0;

        for range in &seq.ops {
            if range.len() == 0 {
                continue;
            }

            let range_base = i64::from(range.ins());
            if range_base > base_cursor {
                doc_cursor += range_base - base_cursor;
                base_cursor = range_base;
            }

            if range.len() > 0 {
                let ins: InsertPos = doc_cursor.try_into().expect("insert cursor overflow");
                match range {
                    Op::Insert { content, .. } => {
                        result.push((
                            range_base,
                            Op::Insert {
                                ins,
                                content: content.clone(),
                            },
                        ));
                    }
                    _ => unreachable!(),
                }
                doc_cursor += i64::from(range.len());
            } else {
                let delete_len = -i64::from(range.len());
                let delete_start = doc_cursor;
                let ins: InsertPos = (delete_start + delete_len)
                    .try_into()
                    .expect("delete cursor overflow");
                let len: Length = delete_len.try_into().expect("delete len overflow");
                result.push((range_base, Op::Delete { ins, len: -len }));
                base_cursor += delete_len;
            }
        }

        result
    }

    /// Applies base-anchored ops to `base` and returns the resulting string.
    /// Example: base "0123" with `[Insert at 2 "X"]` yields "01X23".
    fn apply_base_ops_to_string(base: &str, ops: &[(i64, Op)]) -> String {
        let base_chars: Vec<char> = base.chars().collect();
        let mut result = String::new();
        let mut cursor: usize = 0;

        for (base_pos, op) in ops {
            let target = (*base_pos).max(0).try_into().unwrap_or(usize::MAX);
            let target = target.min(base_chars.len());

            while cursor < target {
                result.push(base_chars[cursor]);
                cursor += 1;
            }

            match op {
                Op::Insert { content, .. } => result.push_str(content),
                Op::Delete { len, .. } => {
                    let delete_len = (-len) as usize;
                    cursor = cursor.saturating_add(delete_len).min(base_chars.len());
                }
            }
        }

        while cursor < base_chars.len() {
            result.push(base_chars[cursor]);
            cursor += 1;
        }

        result
    }

    /// Renders an `OpList` as a document string using the numbered template.
    fn final_doc_from_oplist(oplist: &OpList) -> String {
        const BASE_DOC_SOURCE: &str =
            "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let ops = ops_with_base(oplist);
        apply_base_ops_to_string(BASE_DOC_SOURCE, &ops)
    }

    /// Returns the doc string after merging `other` into `base`.
    fn final_doc_after_merge(base: &OpList, other: &OpList) -> String {
        let mut merged = base.from_oplist_to_sequential_list();
        let seq_other = other.from_oplist_to_sequential_list();
        merged.merge_sequential_list(&seq_other);
        final_doc_from_oplist(&merged)
    }

    /// Returns the doc string after normalizing a transform and replaying the resulting ops.
    fn final_doc_after_normal_apply(base: &OpList, other: &OpList) -> String {
        let seq_base = base.from_oplist_to_sequential_list();
        let mut applied = seq_base.clone();
        let seq_other = other.from_oplist_to_sequential_list();
        let transformed = seq_base.transform(&seq_other);
        let mut normalized_ops = transformed.clone();
        normalized_ops.from_sequential_list_to_oplist();
        for op in &normalized_ops.ops {
            if op.len() > 0 {
                if let Op::Insert { ins, content } = op {
                    OpList::apply_insert(&mut applied.ops, *ins, content.clone());
                }
            } else {
                let start = op.ins() + op.len();
                let len = -op.len();
                OpList::apply_delete(&mut applied.ops, start, len);
            }
        }
        final_doc_from_oplist(&applied)
    }

    #[test]
    /// Ensures every `merge_sequential_list_behaviors` case produces the same document when replayed.
    fn merge_behaviors_preserve_final_document_for_normal_apply() {
        let cases = vec![
            (
                getOpList([TestOp::Ins(5, "AB"), TestOp::Ins(10, "C")]),
                getOpList([TestOp::Ins(5, "DEF"), TestOp::Ins(7, "G")]),
            ),
            (getOpList([(5, -1)]), getOpList([(6, -1)])),
            (
                getOpList([TestOp::Del(3, -1), TestOp::Ins(3, "A"), TestOp::Del(6, -1)]),
                getOpList([(4, -2)]),
            ),
            (getOpList([(5, "A")]), getOpList([(5, -2)])),
            (getOpList([(5, "AB")]), getOpList([(5, "CDE")])),
            (
                getOpList([TestOp::Del(5, -2), TestOp::Ins(5, "A")]),
                getOpList([TestOp::Ins(5, "B"), TestOp::Del(6, -1)]),
            ),
            (
                getOpList([TestOp::Ins(5, "ABC")]),
                getOpList([TestOp::Ins(5, "DE")]),
            ),
            (
                getOpList([TestOp::Ins(5, "ABC")]),
                getOpList([TestOp::Ins(3, "DE")]),
            ),
            (
                getOpList([TestOp::Ins(2, "AB"), TestOp::Ins(6, "CD")]),
                getOpList([TestOp::Ins(4, "XYZ")]),
            ),
            (
                getOpList([TestOp::Ins(5, "AB")]),
                getOpList([TestOp::Del(10, -2)]),
            ),
        ];

        for (existing, additions) in cases {
            let normal_doc = final_doc_after_normal_apply(&existing, &additions);
            let merge_doc = final_doc_after_merge(&existing, &additions);
            let backwards_doc = final_doc_after_backwards_apply(&existing, &additions);
            assert_eq!(
                normal_doc, merge_doc,
                "Normal apply diverged from merge for existing {:?} and additions {:?}",
                existing.ops, additions.ops
            );
            assert_eq!(
                merge_doc, backwards_doc,
                "Backwards apply diverged from merge for existing {:?} and additions {:?}",
                existing.ops, additions.ops
            );
        }
    }

    /// Returns the final document after transforming and backwards-applying `other` on `base`.
    fn final_doc_after_backwards_apply(base: &OpList, other: &OpList) -> String {
        let seq_base = base.from_oplist_to_sequential_list();
        let seq_other = other.from_oplist_to_sequential_list();

        let transformed = seq_base.transform(&seq_other);
        let applied = transformed.backwards_apply(&seq_base);
        final_doc_from_oplist(&applied)
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
                let prior_base = i64::from(prior_op.ins());
                let effective_base = if prior_op.len() > 0 {
                    prior_base
                } else {
                    // For deletes, the effective base is at the end of the deleted range
                    prior_base + i64::from(-prior_op.len())
                };

                if effective_base <= base {
                    let prior_op = prior_ops_iter.next().unwrap();
                    if prior_op.len() > 0 {
                        shift_all += i64::from(prior_op.len());
                    } else {
                        let delete_len = -i64::from(prior_op.len());
                        shift_all += -delete_len;
                        shift_deletes += -delete_len;
                    }
                } else {
                    break;
                }
            }

            if op.len() == 0 {
                continue;
            } else if op.len() > 0 {
                let adjusted = i64::from(op.ins()) + shift_deletes;
                let ins: InsertPos = adjusted.try_into().expect("insert cursor overflow");
                match op {
                    Op::Insert { content, .. } => {
                        OpList::apply_insert(&mut baseline.ops, ins, content);
                    }
                    _ => unreachable!(),
                }
            } else {
                let start = i64::from(op.ins() + op.len()) + shift_deletes;
                let start_pos: InsertPos = start.try_into().expect("delete start overflow");
                let len = -op.len();
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

        let current = getOpList([(3, "A")]);
        let prior = getOpList([(2, "B")]);
        let expected = getOpList([(2, "BA")]);
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
            (
                getOpList([TestOp::Del(3, -2)]),
                getOpList([TestOp::Del(2, -1)]),
            ),
            (
                getOpList([TestOp::Ins(3, "A")]),
                getOpList([TestOp::Ins(2, "B")]),
            ),
            (
                getOpList([TestOp::Del(5, -1), TestOp::Ins(5, "A")]),
                getOpList([TestOp::Ins(4, "B")]),
            ),
            (
                getOpList([TestOp::Del(2, -3), TestOp::Ins(2, "AB"), TestOp::Del(7, -1)]),
                getOpList([TestOp::Ins(6, "CD"), TestOp::Del(9, -2)]),
            ),
            (
                getOpList([
                    TestOp::Ins(1, "AB"),
                    TestOp::Del(4, -1),
                    TestOp::Ins(4, "C"),
                ]),
                getOpList([TestOp::Del(3, -1), TestOp::Ins(5, "D"), TestOp::Del(7, -2)]),
            ),
        ];

        for (current, prior) in cases {
            let current_seq = current.from_oplist_to_sequential_list();
            let prior_seq = prior.from_oplist_to_sequential_list();
            let result = current_seq.backwards_apply(&prior_seq);
            let reference = backwards_apply_reference(&current_seq, &prior_seq);
            assert_eq!(
                result, reference,
                "Reference implementation diverged for current {:?} prior {:?}",
                current_seq.ops, prior_seq.ops
            );
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
        let test_vec: OpList = getOpListforTesting([(5, -1)], [(7, "A")]);
        let expected_result = getOpList([TestOp::Del(5, -1), TestOp::Ins(8, "A")]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-67890...
        // After new applied (+) = 12345-6+7890...
        let test_vec: OpList = getOpListforTesting([(5, "A")], [(7, "B")]);
        let expected_result = getOpList([TestOp::Ins(5, "A"), TestOp::Ins(6, "B")]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Test cases for: 1234567 -> 123456-7= -> 12345-=

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345+-=890...
        let test_vec: OpList = getOpListforTesting(
            [TestOp::Del(5, -2), TestOp::Ins(6, "A"), TestOp::Ins(7, "B")],
            [(5, "C")],
        );
        let expected_result = getOpList([
            TestOp::Ins(5, "C"),
            TestOp::Del(5, -2),
            TestOp::Ins(6, "A"),
            TestOp::Ins(7, "B"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-+=890...
        let test_vec: OpList = getOpListforTesting(
            [TestOp::Del(5, -2), TestOp::Ins(6, "A"), TestOp::Ins(7, "B")],
            [(6, "C")],
        );
        let expected_result = getOpList([
            TestOp::Del(5, -2),
            TestOp::Ins(6, "AC"),
            TestOp::Ins(7, "B"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 12345-=890...
        // After new applied (+) = 12345-=8+90...
        let test_vec: OpList = getOpListforTesting(
            [TestOp::Del(5, -2), TestOp::Ins(6, "A"), TestOp::Ins(7, "B")],
            [(8, "C")],
        );
        let expected_result = getOpList([
            TestOp::Del(5, -2),
            TestOp::Ins(6, "A"),
            TestOp::Ins(7, "B"),
            TestOp::Ins(8, "C"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Test cases for 1-2-35-

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-+35-67890...
        let test_vec: OpList = getOpListforTesting(
            [
                TestOp::Ins(1, "A"),
                TestOp::Ins(2, "B"),
                TestOp::Del(3, -1),
                TestOp::Ins(5, "C"),
            ],
            [(4, "D")],
        );
        let expected_result = getOpList([
            TestOp::Ins(1, "A"),
            TestOp::Ins(2, "BD"),
            TestOp::Del(3, -1),
            TestOp::Ins(5, "C"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-3+5-67890...
        let test_vec: OpList = getOpListforTesting(
            [
                TestOp::Ins(1, "A"),
                TestOp::Ins(2, "B"),
                TestOp::Del(3, -1),
                TestOp::Ins(5, "C"),
            ],
            [(5, "D")],
        );
        let expected_result = getOpList([
            TestOp::Ins(1, "A"),
            TestOp::Ins(2, "B"),
            TestOp::Ins(3, "D"),
            TestOp::Del(3, -1),
            TestOp::Ins(5, "C"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35+-67890...
        let test_vec: OpList = getOpListforTesting(
            [
                TestOp::Ins(1, "A"),
                TestOp::Ins(2, "B"),
                TestOp::Del(3, -1),
                TestOp::Ins(5, "C"),
            ],
            [(6, "D")],
        );
        let expected_result = getOpList([
            TestOp::Ins(1, "A"),
            TestOp::Ins(2, "B"),
            TestOp::Del(3, -1),
            TestOp::Ins(5, "DC"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1-2-35-67890...
        // After new applied (+) = 1-2-35-6+7890...
        let test_vec: OpList = getOpListforTesting(
            [
                TestOp::Ins(1, "A"),
                TestOp::Ins(2, "B"),
                TestOp::Del(3, -1),
                TestOp::Ins(5, "C"),
            ],
            [(8, "D")],
        );
        let expected_result = getOpList([
            TestOp::Ins(1, "A"),
            TestOp::Ins(2, "B"),
            TestOp::Del(3, -1),
            TestOp::Ins(5, "C"),
            TestOp::Ins(6, "D"),
        ]);
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
        let mut test_vec: OpList = getOpListforTesting(
            [
                TestOp::Del(3, -3),
                TestOp::Ins(4, "A"),
                TestOp::Ins(5, "B"),
                TestOp::Ins(6, "C"),
                TestOp::Del(7, -1),
            ],
            [(7, -1)],
        );
        let expected_result = getOpList([
            TestOp::Del(3, -5),
            TestOp::Ins(4, "A"),
            TestOp::Ins(5, "B"),
            TestOp::Ins(6, "C"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 123-=~790...
        // After new applied = 123-90...
        let mut test_vec: OpList = getOpListforTesting(
            [
                TestOp::Del(3, -3),
                TestOp::Ins(4, "A"),
                TestOp::Ins(5, "B"),
                TestOp::Ins(6, "C"),
                TestOp::Del(7, -1),
            ],
            [(7, -3)],
        );
        let expected_result = getOpList([TestOp::Del(3, -5), TestOp::Ins(4, "A")]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Pre existing = 1234567-90...
        // After new applied = 12345690...
        let mut test_vec: OpList =
            getOpListforTesting([TestOp::Ins(7, "A"), TestOp::Del(7, -1)], [(8, -2)]);
        let expected_result = getOpList([(6, -2)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 14678-=~90...
        // Expected = 14678-=~90...
        let mut test_vec: OpList = getOpList([
            TestOp::Del(5, -1),
            TestOp::Del(3, -1),
            TestOp::Ins(6, "ABC"),
            TestOp::Del(2, -1),
        ]); // hard to understand
        let expected_result = getOpList([
            TestOp::Del(1, -2),
            TestOp::Del(4, -1),
            TestOp::Ins(8, "ABC"),
        ]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Base = 1234567890...
        // Test = 127890...
        // Expected = 127890...
        let mut test_vec: OpList = getOpList([(5, -2), (4, -2)]); // 1234567 -> 12367 -> 127; Testing for delete RLE within delete RLE
        let expected_result = getOpList([(2, -4)]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Basic test
        let test_vec: OpList = getOpListforTesting([(5, "ABCDE")], [(7, -2)]);
        let expected_result = getOpList([(5, "CDE")]); // (7, -2) deletes [5, 7) -> "AB" removed.
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // Prepend test
        let test_vec: OpList = getOpListforTesting([(1, "ABC")], [(1, "A")]);
        let expected_result = getOpList([TestOp::Ins(1, "AABC")]);
        assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);

        // // Could be useful
        // // -11-2---
        // let test_vec: OpList = getOpListforTesting([(1,1),(1,1)], [(4,1)]);
        // let expected_result = getOpList([(1,1),(1,1),(2,1)]);
        // assert_eq!(test_vec.from_oplist_to_sequential_list(), expected_result);
    }
    #[test]
    fn test_merge_graph_diamond() {
        // Graph structure:
        //      1(A)
        //     /   \
        //   2(B)   \ (2 also connects to 4? Original request: 1->2, 2->3, 2->4)
        //   /  \    \
        // 3(C) 4(D)  \
        //
        // Wait, request said: 1->2 (1 splits to 2?), 2 splits to 3 and 4?
        // "start->1,2; 1->3,4"
        // Diamond example I implemented:
        // 1 (root) -> 2
        // 2 -> 3
        // 2 -> 4

        let op1 = getOpList([(0, "A")]);
        let mut graph = Graph::new(1, op1);

        let op2 = getOpList([(1, "B")]);
        graph.add_node(2, op2, vec![1]);

        let op3 = getOpList([(2, "C")]);
        graph.add_node(3, op3, vec![2]);

        let op4 = getOpList([(2, "D")]);
        graph.add_node(4, op4, vec![2]);

        let mut final_oplist = graph.merge_graph();
        final_oplist.from_sequential_list_to_oplist();

        // Expected: A -> B -> (C merged D)
        // C and D are siblings at same insertion point (2, relative to AB).
        // Deterministic sort: 3 processed before 4.
        // 3 inserts C at 2.
        // 4 inserts D at 2.
        // Merging 4 into 3: D is inserted at 2.
        // If 3 has "C" at 2. 4 inserts "D" at 2.
        // "D" should be merged. If same position, `merge_insert` uses `ranges.insert(idx, op)`.
        // So D comes before C? Or after?
        // `merge_insert`: "while ranges[idx].ins() < op.ins() ... while ranges[idx].ins() == op.ins() ... insert"
        // It skips existing inserts at same position -> inserts after them?
        // Wait: `while idx < ranges.len() && ranges[idx].ins() == op.ins()` loops over *all* existing inserts at that pos.
        // Then `ranges.insert(idx, op)` -> inserts *after* them (since idx incremented).
        // So D (from 4) will be AFTER C (from 3).
        // Result: A B C D.

        let res = oplist_to_string(&final_oplist);
        assert_eq!(res, "ABCD");
    }

    #[test]
    fn test_merge_shared_parents_siblings() {
        // Test merging multiple siblings to ensure deterministic order and memoization.
        // Root(A) -> 2(B), 3(C), 4(D)
        // Graph:
        //      1(A)
        //    / | \
        //   2  3  4
        // All insert at position 1 (after A).

        let op1 = getOpList([(0, "A")]);
        let mut graph = Graph::new(1, op1); // A

        let op2 = getOpList([(1, "B")]);
        graph.add_node(2, op2, vec![1]);

        let op3 = getOpList([(1, "C")]);
        graph.add_node(3, op3, vec![1]);

        let op4 = getOpList([(1, "D")]);
        graph.add_node(4, op4, vec![1]);

        // Walk 1 calls walk(2), walk(3), walk(4).
        // Result 2: B (at 1)
        // Result 3: C (at 1)
        // Result 4: D (at 1)

        // Merge order: 2 (base) points to B.
        // Merge 3 into 2: C at 1. Existing B at 1. C inserts after B -> BC.
        // Merge 4 into 2: D at 1. Existing B, C at 1. D inserts after C -> BCD.
        // Apply backwards to A -> ABCD.

        let mut final_oplist = graph.merge_graph();
        final_oplist.from_sequential_list_to_oplist();

        let res = oplist_to_string(&final_oplist);
        assert_eq!(res, "ABCD");
    }

    #[test]
    fn test_dag_shared_children() {
        // DAG Structure:
        //      0 (Root)
        //     /  \
        //    1    2
        //    | \/ |
        //    | /\ |
        //    3    4
        //
        // 1 -> 3, 4
        // 2 -> 3, 4

        let op0 = getOpList([(0, "A")]);
        let mut graph = Graph::new(0, op0);

        let op1 = getOpList([(1, "B")]);
        graph.add_node(1, op1, vec![0]);

        let op2 = getOpList([(1, "C")]);
        graph.add_node(2, op2, vec![0]);

        let op3 = getOpList([(2, "D")]);
        graph.add_node(3, op3, vec![1, 2]);

        let op4 = getOpList([(2, "E")]);
        graph.add_node(4, op4, vec![1, 2]);

        // TEST INTERMEDIATE STATES with Deduplication
        // ------------------------------------------
        // Visited set persists across calls if we reuse it.
        // But here we want to check what walk(1) produces in isolation.

        let mut visited = std::collections::HashSet::new();
        // walk(1): Visits 1, then 3, then 4.
        // Result: BDE.
        let res1 = graph.walk(1, &mut visited);
        assert_eq!(oplist_to_string(&res1), "BDE");

        // walk(2): Visits 2.
        // Children 3 and 4 are ALREADY IN VISITED from walk(1).
        // So they return empty.
        // Result: C + empty = C.
        let res2 = graph.walk(2, &mut visited);
        assert_eq!(oplist_to_string(&res2), "C");

        // Full Merge Logic (fresh start)
        // ------------------------------
        // merge_graph() creates a fresh visited set.
        // Root A.
        // Visit 1: returns BDE. (Visited: 0, 1, 3, 4)
        // Visit 2: returns C. (Visited: 0, 1, 3, 4, 2). Children 3, 4 skipped.
        // Merge 1 (BDE) and 2 (C).
        // BDE at 1. C at 1.
        // Deterministic sort: 1 processed before 2.
        // 1 inserts BDE. 2 inserts C.
        // 2 merges into 1. C inserts after BDE?
        // Wait, C is at 1. BDE is at 1.
        // If range ins == op ins, we insert.
        // BDE is inserted. C is inserted.
        // Result: A BDE C. -> ABDEC.

        let final_oplist = graph.merge_graph();
        let res = oplist_to_string(&final_oplist);
        assert_eq!(res, "ABDEC");
    }
}
