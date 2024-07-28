mod main_new_algo;
mod mako_new_algo_multithreading;

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

impl<const INTERNAL_ENT:usize, const LEAF_ENTRIES:usize> NodeInternal<INTERNAL_ENT, LEAF_ENTRIES> {
	// Searches for leaf node with pos.
	fn search(&self, pos:u32, offset:Option<u32>) -> Option<&mut NodeLeaf<INTERNAL_ENT, LEAF_ENTRIES>> {
			let internal = self;
			let mut count: u32 = 0;
			if let Some(offset) = offset {
				count = offset;
			}
			for (i, len_count) in internal.len_counts.iter().enumerate() {
				let next_internal = internal.children[i].as_mut().unwrap();
				if (pos >= count) & (pos < (count + len_count)) {
					match next_internal {
							Node::Internal(internal_child) => {
								return internal_child.search(pos, Some(count))
							},
							Node::Leaf(leaf_child) => {
								return Some(leaf_child as &mut NodeLeaf<INTERNAL_ENT, LEAF_ENTRIES>);
							},
						}
				}
				count += len_count;
			}
			None
			// unreachable!()
	} 
}

impl<const INTERNAL_ENT:usize, const LEAF_ENTRIES:usize> NodeLeaf<INTERNAL_ENT, LEAF_ENTRIES> {
	fn search(self, pos:u32, offset:Option<u32>) -> Option<CRDTEdits> {
			let leaf = self;
			let mut count: u32 = 0;
			if let Some(offset) = offset {
				count = offset;
			}
			for CRDTedits in leaf.data {
				// Check if pos is within the RLE.
				if (pos >= count as u32) & (pos < (count + CRDTedits.RLE_length as u32)) {
					return Some(CRDTedits);
				}
				count += CRDTedits.RLE_length as u32;
			}
			None
			// unreachable!()
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