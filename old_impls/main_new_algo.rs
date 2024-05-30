use std::collections::HashMap;

struct OpLog {
	log: HashMap<ClientId, Vec<Op>>,
	frontiers: [Op],
	first_ops: [Op] // multi copy of Ops in child mergering is invadable, also in frontiers.
}

struct VV {
	vectors: [OpId],
}

enum VVCompareResults {
	Equal,
	Greater,
	Lesser,
}

// vv_index = [ // Ordered VV, binary search, VV is sum of all parents.
// 	[{1: 50, 2:50}],
// 	[{1: 75, 2:37}],
// 	[{1: 100, 2:25}],
// 	[{1: 100, 2:36}],
// ]

impl VV {
	fn cmp(&self, other: &VV) -> bool {
		
	}
}

impl OpLog {
	// we can insert sequencially or concurrently, and sometimes for testing we may need to insert concurrently but then make it sequential to a previously sequential operations.
	// For testing we would need to create random OpLogs and test for correctness, comparing with DT/Loro.
	fn local_insert_at_pos(&mut self, pos: u32, len:u32, user_id: ClientId) {
		let IDLogs = self.log.get_mut(ClientId);
		match IDLogs {
			Some(log) => {
				let last_op = log.last().unwrap();
				log.push(Op {
					OpId: OpId { UserID: user_id, Seq: last_op.OpId.Seq +  }, 
					Pos: pos, 
					Len: len, 
					child: OpId::default(), 
					OpType: OpType::Insert 
				});
			},
			None => {
				self.log.insert(ClientId, vec![Op {
					OpId: OpId { UserID: user_id, Seq: 0 }, 
					Pos: pos, 
					Len: len, 
					child: OpId::default(), 
					OpType: OpType::Insert 
				}]);
			},
		}
		// let OpID = OpId { UserID: user_id,
		// 	Index: IDLogs.last()
		// };
		// IDLogs.append(&mut [Op {
		// 	OpId: OpId { UserID: user_id, Index: 0 }, 
		// 	Pos: pos, 
		// 	Len: len, 
		// 	child: OpId::default(), 
		// 	OpType: OpType::Insert 
		// }]);	
	}

	fn sequential_insertion(&mut self, operations: [Opid], parents: [OpId]) {
		todo!()
	}

	fn concurrent_insertion(&mut self, operations: [Opid], LCA: [OpId]) { // <- LCA should be a list or single element? IG it should be, see notes, as its commited.
		todo!()
	}

	fn frontier(&self) -> [Op] {
		return self.frontiers
	}
}

struct Op {
	OpId: OpId,
	Pos: u32,
	Len: u32,
	// parent/child? Lets assume child as that would be more optimal for the time being.
	child: OpId,
	OpType: OpType
}

// impl Op {
// 	fn update_child(&mut self, child:OpId) {
// 		self.child = child;
// 	}
// }

/// Every client has to have a unique ID, somehow I would need to create largeID -> small ID map.
type ClientId = u32;

type Sequence = u32;

struct OpId {
	UserID: ClientId,
	Seq: Sequence
}

impl OpId {
	fn is_frontier(&self) -> bool {
		if self.UserID == u32::MAX && self.Seq == u32::MAX {
			return true
		} else {
			return false
		}
	}
}

impl Default for OpId {
	fn default() -> Self {
		// this is frontier
		Self { UserID: u32::MAX, Seq: u32::MAX }
	}
}

enum OpType {
	Insert,
	// Delete
}