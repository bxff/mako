use std::{collections::btree_map::Range, marker::PhantomPinned, pin::Pin, ptr::NonNull};

const DEFAULT_IE:usize = 8; const DEFAULT_LE:usize = 8;

struct RangeTree<const INTERNAL_ENT:usize = DEFAULT_IE, const LEAF_ENTRIES:usize = DEFAULT_LE> {
    len: u32,
    root: Node<INTERNAL_ENT, LEAF_ENTRIES>,
    _pin: PhantomPinned, 
}

pub(crate) enum Node<const INTERNAL_ENT:usize = DEFAULT_IE, const LEAF_ENTRIES:usize = DEFAULT_LE> {
    Internal(Pin<Box<NodeInternal<INTERNAL_ENT, LEAF_ENTRIES>>>),
    Leaf(Pin<Box<NodeLeaf<INTERNAL_ENT, LEAF_ENTRIES>>>),
}

struct NodeInternal<const INTERNAL_ENT:usize = DEFAULT_IE, const LEAF_ENTRIES:usize = DEFAULT_LE> {
    parent: ParentPtr<INTERNAL_ENT, LEAF_ENTRIES>,
    // Pairs of (count of subtree elements, subtree contents).
    // Left packed. The nodes are all the same type.
    // ItemCount only includes items which haven't been deleted.
    len_counts: [u32; INTERNAL_ENT],
    children: [Option<Node<INTERNAL_ENT, LEAF_ENTRIES>>; INTERNAL_ENT],
    _pin: PhantomPinned, // Needed because children have parent pointers here.
}


pub struct NodeLeaf<const INT_ENTRIES: usize = DEFAULT_IE, const LEAF_ENTRIES: usize = DEFAULT_LE> {
    parent: ParentPtr<INT_ENTRIES, LEAF_ENTRIES>,
    num_entries: u8, // Number of entries which have been populated
    data: [CRDTEdits; LEAF_ENTRIES],
    _pin: PhantomPinned, // Needed because cursors point here.

    next: Option<NonNull<Self>>,
}

pub(crate) enum ParentPtr<const INT_ENTRIES: usize = DEFAULT_IE, const LEAF_ENTRIES: usize = DEFAULT_LE> {
    Root(NonNull<RangeTree<INT_ENTRIES, LEAF_ENTRIES>>),
    Internal(NonNull<NodeInternal<INT_ENTRIES, LEAF_ENTRIES>>)
}

// [13, 11, 21, 31]
// [c1, c2, c3, c4]

impl<const INTERNAL_ENT:usize, const LEAF_ENTRIES:usize> RangeTree<INTERNAL_ENT, LEAF_ENTRIES> {
    fn search(self, pos:u32, offset:Option<u32>, internal_node:Option<Node<INTERNAL_ENT, LEAF_ENTRIES>>) -> Option<CRDTEdits> {
            let root;
            match internal_node {
                Some(node) => {
                    root = node
                }
                None => {
                    root = self.root;
                }
            }
            match root {
                Node::Internal(mut internal) => {
                    let mut count: u32 = 0;
                    match offset {
                        Some(offset) => {
                            count = offset;
                        }
                        None => {}
                    }
                    for (i, len_count) in internal.len_counts.iter().enumerate() {
                        // let mut next_internal = internal.children[i].unwrap();
                        if (pos >= count.try_into().unwrap()) & (pos < (count + len_count).try_into().unwrap()) {
                            return self.search(pos, Some(count), Some(internal.children[i].unwrap()));
                        }
                        count += len_count;
                    }
                    None
                    // unreachable!()
                }
                Node::Leaf(mut leaf) => {
                    let mut count: i32 = 0;
                    for CRDTedits in leaf.data {
                        // Check if pos is within the RLE.
                        if (pos >= count.try_into().unwrap()) & (pos < (count + CRDTedits.RLE_length).try_into().unwrap()) {
                            return Some(CRDTedits);
                        }
                        count += CRDTedits.RLE_length;
                    }
                    None
                }
            }
    } 
}

#[derive(Copy, Clone)]
struct CRDTEdits {
    ID: OpID,
    LO: OpID,
    RLE_length: i32,
    RO: OpID
}

#[derive(Copy, Clone)]
struct OpID {
    UserID: i32,
    seq: u32
}